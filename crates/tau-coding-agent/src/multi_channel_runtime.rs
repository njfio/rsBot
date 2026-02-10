use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::channel_store::{ChannelContextEntry, ChannelLogEntry, ChannelStore};
use crate::multi_channel_contract::{
    event_contract_key, load_multi_channel_contract_fixture, MultiChannelContractFixture,
    MultiChannelEventKind, MultiChannelInboundEvent,
};
use crate::multi_channel_live_ingress::parse_multi_channel_live_inbound_envelope;
use crate::{current_unix_timestamp_ms, write_text_atomic, TransportHealthSnapshot};

const MULTI_CHANNEL_RUNTIME_STATE_SCHEMA_VERSION: u32 = 1;
const MULTI_CHANNEL_RUNTIME_EVENTS_LOG_FILE: &str = "runtime-events.jsonl";
const MULTI_CHANNEL_LIVE_INGRESS_SOURCES: [(&str, &str); 3] = [
    ("telegram", "telegram.ndjson"),
    ("discord", "discord.ndjson"),
    ("whatsapp", "whatsapp.ndjson"),
];

fn multi_channel_runtime_state_schema_version() -> u32 {
    MULTI_CHANNEL_RUNTIME_STATE_SCHEMA_VERSION
}

#[derive(Debug, Clone)]
pub(crate) struct MultiChannelRuntimeConfig {
    pub(crate) fixture_path: PathBuf,
    pub(crate) state_dir: PathBuf,
    pub(crate) queue_limit: usize,
    pub(crate) processed_event_cap: usize,
    pub(crate) retry_max_attempts: usize,
    pub(crate) retry_base_delay_ms: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct MultiChannelLiveRuntimeConfig {
    pub(crate) ingress_dir: PathBuf,
    pub(crate) state_dir: PathBuf,
    pub(crate) queue_limit: usize,
    pub(crate) processed_event_cap: usize,
    pub(crate) retry_max_attempts: usize,
    pub(crate) retry_base_delay_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct MultiChannelRuntimeSummary {
    pub(crate) discovered_events: usize,
    pub(crate) queued_events: usize,
    pub(crate) completed_events: usize,
    pub(crate) duplicate_skips: usize,
    pub(crate) transient_failures: usize,
    pub(crate) retry_attempts: usize,
    pub(crate) failed_events: usize,
}

#[derive(Debug, Clone, Serialize)]
struct MultiChannelRuntimeCycleReport {
    timestamp_unix_ms: u64,
    health_state: String,
    health_reason: String,
    reason_codes: Vec<String>,
    discovered_events: usize,
    queued_events: usize,
    completed_events: usize,
    duplicate_skips: usize,
    transient_failures: usize,
    retry_attempts: usize,
    failed_events: usize,
    backlog_events: usize,
    failure_streak: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MultiChannelRuntimeState {
    #[serde(default = "multi_channel_runtime_state_schema_version")]
    schema_version: u32,
    #[serde(default)]
    processed_event_keys: Vec<String>,
    #[serde(default)]
    health: TransportHealthSnapshot,
}

impl Default for MultiChannelRuntimeState {
    fn default() -> Self {
        Self {
            schema_version: MULTI_CHANNEL_RUNTIME_STATE_SCHEMA_VERSION,
            processed_event_keys: Vec::new(),
            health: TransportHealthSnapshot::default(),
        }
    }
}

pub(crate) async fn run_multi_channel_contract_runner(
    config: MultiChannelRuntimeConfig,
) -> Result<()> {
    let fixture = load_multi_channel_contract_fixture(&config.fixture_path)?;
    let mut runtime = MultiChannelRuntime::new(config)?;
    let summary = runtime.run_once_fixture(&fixture).await?;
    let health = runtime.transport_health().clone();
    let classification = health.classify();
    println!(
        "multi-channel runner summary: discovered={} queued={} completed={} duplicate_skips={} retries={} transient_failures={} failed={}",
        summary.discovered_events,
        summary.queued_events,
        summary.completed_events,
        summary.duplicate_skips,
        summary.retry_attempts,
        summary.transient_failures,
        summary.failed_events
    );
    println!(
        "multi-channel runner health: state={} failure_streak={} queue_depth={} reason={}",
        classification.state.as_str(),
        health.failure_streak,
        health.queue_depth,
        classification.reason
    );
    Ok(())
}

pub(crate) async fn run_multi_channel_live_runner(
    config: MultiChannelLiveRuntimeConfig,
) -> Result<()> {
    let live_events = load_multi_channel_live_events(&config.ingress_dir)?;
    let mut runtime = MultiChannelRuntime::new(MultiChannelRuntimeConfig {
        fixture_path: config.ingress_dir.join("live-ingress.ndjson"),
        state_dir: config.state_dir.clone(),
        queue_limit: config.queue_limit,
        processed_event_cap: config.processed_event_cap,
        retry_max_attempts: config.retry_max_attempts,
        retry_base_delay_ms: config.retry_base_delay_ms,
    })?;
    let summary = runtime.run_once_events(&live_events).await?;
    let health = runtime.transport_health().clone();
    let classification = health.classify();
    println!(
        "multi-channel live runner summary: discovered={} queued={} completed={} duplicate_skips={} retries={} transient_failures={} failed={}",
        summary.discovered_events,
        summary.queued_events,
        summary.completed_events,
        summary.duplicate_skips,
        summary.retry_attempts,
        summary.transient_failures,
        summary.failed_events
    );
    println!(
        "multi-channel live runner health: state={} failure_streak={} queue_depth={} reason={}",
        classification.state.as_str(),
        health.failure_streak,
        health.queue_depth,
        classification.reason
    );
    Ok(())
}

struct MultiChannelRuntime {
    config: MultiChannelRuntimeConfig,
    state: MultiChannelRuntimeState,
    processed_event_keys: HashSet<String>,
}

impl MultiChannelRuntime {
    fn new(config: MultiChannelRuntimeConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.state_dir)
            .with_context(|| format!("failed to create {}", config.state_dir.display()))?;
        let mut state = load_multi_channel_runtime_state(&config.state_dir.join("state.json"))?;
        state.processed_event_keys =
            normalize_processed_keys(&state.processed_event_keys, config.processed_event_cap);
        let processed_event_keys = state.processed_event_keys.iter().cloned().collect();
        Ok(Self {
            config,
            state,
            processed_event_keys,
        })
    }

    fn state_path(&self) -> PathBuf {
        self.config.state_dir.join("state.json")
    }

    fn transport_health(&self) -> &TransportHealthSnapshot {
        &self.state.health
    }

    async fn run_once_fixture(
        &mut self,
        fixture: &MultiChannelContractFixture,
    ) -> Result<MultiChannelRuntimeSummary> {
        self.run_once_events(&fixture.events).await
    }

    async fn run_once_events(
        &mut self,
        source_events: &[MultiChannelInboundEvent],
    ) -> Result<MultiChannelRuntimeSummary> {
        let cycle_started = Instant::now();
        let mut summary = MultiChannelRuntimeSummary {
            discovered_events: source_events.len(),
            ..MultiChannelRuntimeSummary::default()
        };

        let mut queued_events = source_events.to_vec();
        queued_events.sort_by(|left, right| {
            left.timestamp_ms
                .cmp(&right.timestamp_ms)
                .then_with(|| event_contract_key(left).cmp(&event_contract_key(right)))
        });
        queued_events.truncate(self.config.queue_limit);
        summary.queued_events = queued_events.len();

        for event in queued_events {
            let event_key = event_contract_key(&event);
            if self.processed_event_keys.contains(&event_key) {
                summary.duplicate_skips = summary.duplicate_skips.saturating_add(1);
                continue;
            }

            let simulated_transient_failures = simulated_transient_failures(&event);
            let mut attempt = 1usize;
            loop {
                if attempt <= simulated_transient_failures {
                    summary.transient_failures = summary.transient_failures.saturating_add(1);
                    if attempt >= self.config.retry_max_attempts {
                        summary.failed_events = summary.failed_events.saturating_add(1);
                        break;
                    }
                    summary.retry_attempts = summary.retry_attempts.saturating_add(1);
                    apply_retry_delay(self.config.retry_base_delay_ms, attempt).await;
                    attempt = attempt.saturating_add(1);
                    continue;
                }

                match self.persist_event(&event, &event_key) {
                    Ok(()) => {
                        self.record_processed_event(&event_key);
                        summary.completed_events = summary.completed_events.saturating_add(1);
                        break;
                    }
                    Err(error) => {
                        if attempt >= self.config.retry_max_attempts {
                            eprintln!(
                                "multi-channel runner event failed: key={} transport={} error={error}",
                                event_key,
                                event.transport.as_str()
                            );
                            summary.failed_events = summary.failed_events.saturating_add(1);
                            break;
                        }
                        summary.transient_failures = summary.transient_failures.saturating_add(1);
                        summary.retry_attempts = summary.retry_attempts.saturating_add(1);
                        apply_retry_delay(self.config.retry_base_delay_ms, attempt).await;
                        attempt = attempt.saturating_add(1);
                    }
                }
            }
        }

        let cycle_duration_ms =
            u64::try_from(cycle_started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let health = build_transport_health_snapshot(
            &summary,
            cycle_duration_ms,
            self.state.health.failure_streak,
        );
        let classification = health.classify();
        let reason_codes = cycle_reason_codes(&summary);
        self.state.health = health.clone();

        save_multi_channel_runtime_state(&self.state_path(), &self.state)?;
        append_multi_channel_cycle_report(
            &self
                .config
                .state_dir
                .join(MULTI_CHANNEL_RUNTIME_EVENTS_LOG_FILE),
            &summary,
            &health,
            &classification.reason,
            &reason_codes,
        )?;
        Ok(summary)
    }

    fn persist_event(&self, event: &MultiChannelInboundEvent, event_key: &str) -> Result<()> {
        let store = ChannelStore::open(
            &self.config.state_dir.join("channel-store"),
            event.transport.as_str(),
            &event.conversation_id,
        )?;
        let timestamp_unix_ms = current_unix_timestamp_ms();

        store.append_log_entry(&ChannelLogEntry {
            timestamp_unix_ms,
            direction: "inbound".to_string(),
            event_key: Some(event_key.to_string()),
            source: event.transport.as_str().to_string(),
            payload: serde_json::to_value(event).context("serialize inbound event payload")?,
        })?;

        if !event.text.trim().is_empty() {
            store.append_context_entry(&ChannelContextEntry {
                timestamp_unix_ms,
                role: "user".to_string(),
                text: event.text.trim().to_string(),
            })?;
        }

        let response_text = render_response(event);
        store.append_log_entry(&ChannelLogEntry {
            timestamp_unix_ms: current_unix_timestamp_ms(),
            direction: "outbound".to_string(),
            event_key: Some(event_key.to_string()),
            source: "tau-multi-channel-runner".to_string(),
            payload: json!({
                "response": response_text,
                "event_key": event_key,
                "transport": event.transport.as_str(),
            }),
        })?;
        store.append_context_entry(&ChannelContextEntry {
            timestamp_unix_ms: current_unix_timestamp_ms(),
            role: "assistant".to_string(),
            text: response_text,
        })?;

        Ok(())
    }

    fn record_processed_event(&mut self, event_key: &str) {
        if self.processed_event_keys.contains(event_key) {
            return;
        }
        self.state.processed_event_keys.push(event_key.to_string());
        self.processed_event_keys.insert(event_key.to_string());
        if self.state.processed_event_keys.len() > self.config.processed_event_cap {
            let overflow = self
                .state
                .processed_event_keys
                .len()
                .saturating_sub(self.config.processed_event_cap);
            let removed = self.state.processed_event_keys.drain(0..overflow);
            for key in removed {
                self.processed_event_keys.remove(&key);
            }
        }
    }
}

fn load_multi_channel_live_events(ingress_dir: &Path) -> Result<Vec<MultiChannelInboundEvent>> {
    std::fs::create_dir_all(ingress_dir)
        .with_context(|| format!("failed to create {}", ingress_dir.display()))?;
    let mut events = Vec::new();
    for (transport, file_name) in MULTI_CHANNEL_LIVE_INGRESS_SOURCES {
        let path = ingress_dir.join(file_name);
        if !path.exists() {
            continue;
        }
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        for (index, line) in raw.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match parse_multi_channel_live_inbound_envelope(trimmed) {
                Ok(event) => {
                    if event.transport.as_str() != transport {
                        eprintln!(
                            "multi-channel live ingress skipped event: file={} line={} reason=transport_mismatch expected={} actual={}",
                            path.display(),
                            index + 1,
                            transport,
                            event.transport.as_str()
                        );
                        continue;
                    }
                    events.push(event);
                }
                Err(error) => {
                    eprintln!(
                        "multi-channel live ingress parse failure: file={} line={} reason_code={} detail={}",
                        path.display(),
                        index + 1,
                        error.code.as_str(),
                        error.message
                    );
                }
            }
        }
    }
    Ok(events)
}

fn build_transport_health_snapshot(
    summary: &MultiChannelRuntimeSummary,
    cycle_duration_ms: u64,
    previous_failure_streak: usize,
) -> TransportHealthSnapshot {
    let backlog_events = summary
        .discovered_events
        .saturating_sub(summary.queued_events);
    let failure_streak = if summary.failed_events > 0 {
        previous_failure_streak.saturating_add(1)
    } else {
        0
    };
    TransportHealthSnapshot {
        updated_unix_ms: current_unix_timestamp_ms(),
        cycle_duration_ms,
        queue_depth: backlog_events,
        active_runs: 0,
        failure_streak,
        last_cycle_discovered: summary.discovered_events,
        last_cycle_processed: summary
            .completed_events
            .saturating_add(summary.failed_events)
            .saturating_add(summary.duplicate_skips),
        last_cycle_completed: summary.completed_events,
        last_cycle_failed: summary.failed_events,
        last_cycle_duplicates: summary.duplicate_skips,
    }
}

fn cycle_reason_codes(summary: &MultiChannelRuntimeSummary) -> Vec<String> {
    let mut codes = Vec::new();
    if summary.discovered_events > summary.queued_events {
        codes.push("queue_backpressure_applied".to_string());
    }
    if summary.duplicate_skips > 0 {
        codes.push("duplicate_events_skipped".to_string());
    }
    if summary.retry_attempts > 0 {
        codes.push("retry_attempted".to_string());
    }
    if summary.transient_failures > 0 {
        codes.push("transient_failures_observed".to_string());
    }
    if summary.failed_events > 0 {
        codes.push("event_processing_failed".to_string());
    }
    if codes.is_empty() {
        codes.push("healthy_cycle".to_string());
    }
    codes
}

fn append_multi_channel_cycle_report(
    path: &Path,
    summary: &MultiChannelRuntimeSummary,
    health: &TransportHealthSnapshot,
    health_reason: &str,
    reason_codes: &[String],
) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let payload = MultiChannelRuntimeCycleReport {
        timestamp_unix_ms: current_unix_timestamp_ms(),
        health_state: health.classify().state.as_str().to_string(),
        health_reason: health_reason.to_string(),
        reason_codes: reason_codes.to_vec(),
        discovered_events: summary.discovered_events,
        queued_events: summary.queued_events,
        completed_events: summary.completed_events,
        duplicate_skips: summary.duplicate_skips,
        transient_failures: summary.transient_failures,
        retry_attempts: summary.retry_attempts,
        failed_events: summary.failed_events,
        backlog_events: summary
            .discovered_events
            .saturating_sub(summary.queued_events),
        failure_streak: health.failure_streak,
    };
    let line = serde_json::to_string(&payload).context("serialize multi-channel runtime report")?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    writeln!(file, "{line}").with_context(|| format!("failed to append {}", path.display()))?;
    file.flush()
        .with_context(|| format!("failed to flush {}", path.display()))?;
    Ok(())
}

fn render_response(event: &MultiChannelInboundEvent) -> String {
    let transport = event.transport.as_str();
    let event_id = event.event_id.trim();
    if matches!(event.event_kind, MultiChannelEventKind::Command)
        || event.text.trim().starts_with('/')
    {
        return format!(
            "command acknowledged: transport={} event_id={} conversation={}",
            transport, event_id, event.conversation_id
        );
    }
    format!(
        "message processed: transport={} event_id={} text_chars={}",
        transport,
        event_id,
        event.text.chars().count()
    )
}

fn normalize_processed_keys(raw: &[String], cap: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for key in raw {
        let trimmed = key.trim();
        if trimmed.is_empty() {
            continue;
        }
        let owned = trimmed.to_string();
        if seen.insert(owned.clone()) {
            normalized.push(owned);
        }
    }
    if cap == 0 {
        return Vec::new();
    }
    if normalized.len() > cap {
        normalized.drain(0..normalized.len().saturating_sub(cap));
    }
    normalized
}

fn simulated_transient_failures(event: &MultiChannelInboundEvent) -> usize {
    event
        .metadata
        .get("simulate_transient_failures")
        .and_then(|value| value.as_u64())
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0)
}

fn retry_delay_ms(base_delay_ms: u64, attempt: usize) -> u64 {
    if base_delay_ms == 0 {
        return 0;
    }
    let exponent = attempt.saturating_sub(1).min(10) as u32;
    base_delay_ms.saturating_mul(1_u64 << exponent)
}

async fn apply_retry_delay(base_delay_ms: u64, attempt: usize) {
    let delay_ms = retry_delay_ms(base_delay_ms, attempt);
    if delay_ms > 0 {
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }
}

fn load_multi_channel_runtime_state(path: &Path) -> Result<MultiChannelRuntimeState> {
    if !path.exists() {
        return Ok(MultiChannelRuntimeState::default());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let parsed = match serde_json::from_str::<MultiChannelRuntimeState>(&raw) {
        Ok(state) => state,
        Err(error) => {
            eprintln!(
                "multi-channel runner: failed to parse state file {} ({error}); starting fresh",
                path.display()
            );
            return Ok(MultiChannelRuntimeState::default());
        }
    };
    if parsed.schema_version != MULTI_CHANNEL_RUNTIME_STATE_SCHEMA_VERSION {
        eprintln!(
            "multi-channel runner: unsupported state schema {} in {}; starting fresh",
            parsed.schema_version,
            path.display()
        );
        return Ok(MultiChannelRuntimeState::default());
    }
    Ok(parsed)
}

fn save_multi_channel_runtime_state(path: &Path, state: &MultiChannelRuntimeState) -> Result<()> {
    let payload = serde_json::to_string_pretty(state).context("serialize multi-channel state")?;
    write_text_atomic(path, &payload).with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use serde_json::Value;
    use tempfile::tempdir;

    use super::{
        load_multi_channel_live_events, load_multi_channel_runtime_state, retry_delay_ms,
        run_multi_channel_live_runner, MultiChannelLiveRuntimeConfig, MultiChannelRuntime,
        MultiChannelRuntimeConfig,
    };
    use crate::channel_store::ChannelStore;
    use crate::multi_channel_contract::{
        load_multi_channel_contract_fixture, parse_multi_channel_contract_fixture,
    };
    use crate::transport_health::TransportHealthState;

    fn fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("multi-channel-contract")
            .join(name)
    }

    fn live_fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("multi-channel-live-ingress")
            .join(name)
    }

    fn build_config(root: &Path) -> MultiChannelRuntimeConfig {
        MultiChannelRuntimeConfig {
            fixture_path: fixture_path("baseline-three-channel.json"),
            state_dir: root.join(".tau/multi-channel"),
            queue_limit: 64,
            processed_event_cap: 10_000,
            retry_max_attempts: 3,
            retry_base_delay_ms: 0,
        }
    }

    fn build_live_config(root: &Path) -> MultiChannelLiveRuntimeConfig {
        MultiChannelLiveRuntimeConfig {
            ingress_dir: root.join(".tau/multi-channel/live-ingress"),
            state_dir: root.join(".tau/multi-channel"),
            queue_limit: 64,
            processed_event_cap: 10_000,
            retry_max_attempts: 3,
            retry_base_delay_ms: 0,
        }
    }

    fn write_live_ingress_file(ingress_dir: &Path, transport: &str, fixture_name: &str) {
        std::fs::create_dir_all(ingress_dir).expect("create ingress directory");
        let file_name = format!("{transport}.ndjson");
        let fixture_raw =
            std::fs::read_to_string(live_fixture_path(fixture_name)).expect("read live fixture");
        let fixture_json: Value = serde_json::from_str(&fixture_raw).expect("parse fixture json");
        let fixture_line = serde_json::to_string(&fixture_json).expect("serialize fixture line");
        std::fs::write(ingress_dir.join(file_name), format!("{fixture_line}\n"))
            .expect("write ingress file");
    }

    #[test]
    fn unit_retry_delay_ms_scales_with_attempt_number() {
        assert_eq!(retry_delay_ms(0, 1), 0);
        assert_eq!(retry_delay_ms(10, 1), 10);
        assert_eq!(retry_delay_ms(10, 2), 20);
        assert_eq!(retry_delay_ms(10, 3), 40);
    }

    #[tokio::test]
    async fn functional_runner_processes_fixture_and_persists_channel_store_entries() {
        let temp = tempdir().expect("tempdir");
        let config = build_config(temp.path());
        let fixture =
            load_multi_channel_contract_fixture(&config.fixture_path).expect("fixture should load");
        let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once_fixture(&fixture).await.expect("run once");

        assert_eq!(summary.discovered_events, 3);
        assert_eq!(summary.queued_events, 3);
        assert_eq!(summary.completed_events, 3);
        assert_eq!(summary.duplicate_skips, 0);
        assert_eq!(summary.failed_events, 0);

        let state = load_multi_channel_runtime_state(&config.state_dir.join("state.json"))
            .expect("load state");
        assert_eq!(state.health.last_cycle_discovered, 3);
        assert_eq!(state.health.last_cycle_completed, 3);
        assert_eq!(state.health.last_cycle_failed, 0);
        assert_eq!(state.health.failure_streak, 0);
        assert_eq!(state.health.classify().state, TransportHealthState::Healthy);

        let events_log = std::fs::read_to_string(
            config
                .state_dir
                .join(super::MULTI_CHANNEL_RUNTIME_EVENTS_LOG_FILE),
        )
        .expect("read runtime events log");
        assert!(events_log.contains("healthy_cycle"));
        assert!(events_log.contains("\"health_state\":\"healthy\""));

        for event in &fixture.events {
            let store = ChannelStore::open(
                &config.state_dir.join("channel-store"),
                event.transport.as_str(),
                &event.conversation_id,
            )
            .expect("open store");
            let logs = store.load_log_entries().expect("load logs");
            let context = store.load_context_entries().expect("load context");
            assert_eq!(logs.len(), 2);
            assert!(context.len() >= 2);
        }
    }

    #[tokio::test]
    async fn integration_runner_retries_transient_failure_then_recovers() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.retry_max_attempts = 4;
        let fixture_raw = r#"{
  "schema_version": 1,
  "name": "transient-retry",
  "events": [
    {
      "schema_version": 1,
      "transport": "telegram",
      "event_kind": "message",
      "event_id": "tg-transient-1",
      "conversation_id": "telegram-chat-transient",
      "actor_id": "telegram-user-1",
      "timestamp_ms": 1760100000000,
      "text": "hello",
      "metadata": { "simulate_transient_failures": 1 }
    }
  ]
}"#;
        let fixture = parse_multi_channel_contract_fixture(fixture_raw).expect("parse fixture");
        let mut runtime = MultiChannelRuntime::new(config).expect("runtime");
        let summary = runtime.run_once_fixture(&fixture).await.expect("run once");

        assert_eq!(summary.completed_events, 1);
        assert_eq!(summary.transient_failures, 1);
        assert_eq!(summary.retry_attempts, 1);
        assert_eq!(summary.failed_events, 0);
    }

    #[tokio::test]
    async fn integration_runner_respects_queue_limit_for_backpressure() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.queue_limit = 2;
        let fixture =
            load_multi_channel_contract_fixture(&config.fixture_path).expect("fixture should load");
        let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once_fixture(&fixture).await.expect("run once");

        assert_eq!(summary.discovered_events, 3);
        assert_eq!(summary.queued_events, 2);
        assert_eq!(summary.completed_events, 2);
        let state = load_multi_channel_runtime_state(&config.state_dir.join("state.json"))
            .expect("load state");
        assert_eq!(state.processed_event_keys.len(), 2);
    }

    #[tokio::test]
    async fn regression_runner_skips_duplicate_events_from_persisted_state() {
        let temp = tempdir().expect("tempdir");
        let config = build_config(temp.path());
        let fixture =
            load_multi_channel_contract_fixture(&config.fixture_path).expect("fixture should load");

        let mut first_runtime = MultiChannelRuntime::new(config.clone()).expect("first runtime");
        let first_summary = first_runtime
            .run_once_fixture(&fixture)
            .await
            .expect("first run");
        assert_eq!(first_summary.completed_events, 3);

        let mut second_runtime = MultiChannelRuntime::new(config).expect("second runtime");
        let second_summary = second_runtime
            .run_once_fixture(&fixture)
            .await
            .expect("second run");
        assert_eq!(second_summary.completed_events, 0);
        assert_eq!(second_summary.duplicate_skips, 3);
    }

    #[tokio::test]
    async fn integration_runner_failure_streak_increments_and_resets_on_successful_cycle() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.retry_max_attempts = 2;

        let failing_fixture_raw = r#"{
  "schema_version": 1,
  "name": "persistent-failure",
  "events": [
    {
      "schema_version": 1,
      "transport": "discord",
      "event_kind": "message",
      "event_id": "discord-failing-1",
      "conversation_id": "discord-channel-failing",
      "actor_id": "discord-user-1",
      "timestamp_ms": 1760200000000,
      "text": "retry me",
      "metadata": { "simulate_transient_failures": 5 }
    }
  ]
}"#;
        let failing_fixture = parse_multi_channel_contract_fixture(failing_fixture_raw)
            .expect("parse failing fixture");
        let success_fixture = load_multi_channel_contract_fixture(&config.fixture_path)
            .expect("load success fixture");

        let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
        let first_failed = runtime
            .run_once_fixture(&failing_fixture)
            .await
            .expect("first failed cycle");
        assert_eq!(first_failed.failed_events, 1);
        let state_after_first =
            load_multi_channel_runtime_state(&config.state_dir.join("state.json"))
                .expect("state first");
        assert_eq!(state_after_first.health.failure_streak, 1);
        assert_eq!(
            state_after_first.health.classify().state,
            TransportHealthState::Degraded
        );

        let second_failed = runtime
            .run_once_fixture(&failing_fixture)
            .await
            .expect("second failed cycle");
        assert_eq!(second_failed.failed_events, 1);
        let state_after_second =
            load_multi_channel_runtime_state(&config.state_dir.join("state.json"))
                .expect("state second");
        assert_eq!(state_after_second.health.failure_streak, 2);

        let third_failed = runtime
            .run_once_fixture(&failing_fixture)
            .await
            .expect("third failed cycle");
        assert_eq!(third_failed.failed_events, 1);
        let state_after_third =
            load_multi_channel_runtime_state(&config.state_dir.join("state.json"))
                .expect("state third");
        assert_eq!(state_after_third.health.failure_streak, 3);
        assert_eq!(
            state_after_third.health.classify().state,
            TransportHealthState::Failing
        );

        let success = runtime
            .run_once_fixture(&success_fixture)
            .await
            .expect("successful cycle");
        assert_eq!(success.failed_events, 0);
        assert_eq!(success.completed_events, 3);
        let state_after_success =
            load_multi_channel_runtime_state(&config.state_dir.join("state.json"))
                .expect("state success");
        assert_eq!(state_after_success.health.failure_streak, 0);
        assert_eq!(
            state_after_success.health.classify().state,
            TransportHealthState::Healthy
        );
    }

    #[tokio::test]
    async fn regression_runner_emits_reason_codes_for_failed_cycles() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.retry_max_attempts = 2;
        let failing_fixture_raw = r#"{
  "schema_version": 1,
  "name": "failed-cycle-reasons",
  "events": [
    {
      "schema_version": 1,
      "transport": "whatsapp",
      "event_kind": "message",
      "event_id": "whatsapp-failing-1",
      "conversation_id": "whatsapp-chat-failing",
      "actor_id": "whatsapp-user-1",
      "timestamp_ms": 1760300000000,
      "text": "retries",
      "metadata": { "simulate_transient_failures": 5 }
    }
  ]
}"#;
        let failing_fixture =
            parse_multi_channel_contract_fixture(failing_fixture_raw).expect("parse fixture");

        let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
        let summary = runtime
            .run_once_fixture(&failing_fixture)
            .await
            .expect("run once");
        assert_eq!(summary.failed_events, 1);
        assert_eq!(summary.retry_attempts, 1);

        let events_log = std::fs::read_to_string(
            config
                .state_dir
                .join(super::MULTI_CHANNEL_RUNTIME_EVENTS_LOG_FILE),
        )
        .expect("read runtime events log");
        let first_line = events_log.lines().next().expect("at least one report line");
        let report: Value = serde_json::from_str(first_line).expect("parse report");
        assert_eq!(report["health_state"], "degraded");
        let reason_codes = report["reason_codes"]
            .as_array()
            .expect("reason code array");
        let reason_codes_set = reason_codes
            .iter()
            .filter_map(|value| value.as_str())
            .collect::<std::collections::HashSet<_>>();
        assert!(reason_codes_set.contains("retry_attempted"));
        assert!(reason_codes_set.contains("transient_failures_observed"));
        assert!(reason_codes_set.contains("event_processing_failed"));
    }

    #[test]
    fn unit_live_ingress_loader_skips_invalid_rows_without_failing() {
        let temp = tempdir().expect("tempdir");
        let ingress_dir = temp.path().join("live");
        std::fs::create_dir_all(&ingress_dir).expect("create ingress dir");
        let telegram_raw =
            std::fs::read_to_string(live_fixture_path("telegram-valid.json")).expect("fixture");
        let telegram_json: Value = serde_json::from_str(&telegram_raw).expect("parse fixture");
        let telegram_line = serde_json::to_string(&telegram_json).expect("serialize fixture");
        std::fs::write(
            ingress_dir.join("telegram.ndjson"),
            format!("{telegram_line}\n{{\"transport\":\"slack\"}}\n"),
        )
        .expect("write telegram ingress");

        let events = load_multi_channel_live_events(&ingress_dir).expect("load live events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].transport.as_str(), "telegram");
    }

    #[tokio::test]
    async fn functional_live_runner_processes_ingress_files_and_persists_state() {
        let temp = tempdir().expect("tempdir");
        let config = build_live_config(temp.path());
        write_live_ingress_file(&config.ingress_dir, "telegram", "telegram-valid.json");
        write_live_ingress_file(&config.ingress_dir, "discord", "discord-valid.json");
        write_live_ingress_file(&config.ingress_dir, "whatsapp", "whatsapp-valid.json");

        run_multi_channel_live_runner(config.clone())
            .await
            .expect("live runner should succeed");

        let state =
            load_multi_channel_runtime_state(&config.state_dir.join("state.json")).expect("state");
        assert_eq!(state.health.last_cycle_discovered, 3);
        assert_eq!(state.health.last_cycle_completed, 3);
        assert_eq!(state.health.last_cycle_failed, 0);

        let events_log = std::fs::read_to_string(
            config
                .state_dir
                .join(super::MULTI_CHANNEL_RUNTIME_EVENTS_LOG_FILE),
        )
        .expect("read runtime events log");
        assert!(events_log.contains("\"health_state\":\"healthy\""));
    }

    #[tokio::test]
    async fn integration_live_runner_is_idempotent_across_repeated_cycles() {
        let temp = tempdir().expect("tempdir");
        let config = build_live_config(temp.path());
        write_live_ingress_file(&config.ingress_dir, "telegram", "telegram-valid.json");
        write_live_ingress_file(&config.ingress_dir, "discord", "discord-valid.json");

        run_multi_channel_live_runner(config.clone())
            .await
            .expect("first live run should succeed");
        run_multi_channel_live_runner(config.clone())
            .await
            .expect("second live run should succeed");

        let state =
            load_multi_channel_runtime_state(&config.state_dir.join("state.json")).expect("state");
        assert_eq!(state.processed_event_keys.len(), 2);

        let channel_store_root = config.state_dir.join("channel-store");
        let telegram_store = ChannelStore::open(&channel_store_root, "telegram", "chat-100")
            .expect("open telegram store");
        let telegram_logs = telegram_store.load_log_entries().expect("telegram logs");
        assert_eq!(telegram_logs.len(), 2);
    }

    #[tokio::test]
    async fn regression_live_runner_handles_invalid_transport_file_contents() {
        let temp = tempdir().expect("tempdir");
        let config = build_live_config(temp.path());
        write_live_ingress_file(&config.ingress_dir, "telegram", "telegram-valid.json");
        std::fs::write(
            config.ingress_dir.join("discord.ndjson"),
            "{\"schema_version\":1,\"transport\":\"telegram\",\"provider\":\"telegram-bot-api\",\"payload\":{}}\n",
        )
        .expect("write mismatched ingress");

        run_multi_channel_live_runner(config.clone())
            .await
            .expect("live runner should continue despite mismatch");

        let state =
            load_multi_channel_runtime_state(&config.state_dir.join("state.json")).expect("state");
        assert_eq!(state.health.last_cycle_discovered, 1);
        assert_eq!(state.health.last_cycle_completed, 1);
    }
}
