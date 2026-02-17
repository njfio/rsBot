use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::voice_contract::{
    evaluate_voice_case, load_voice_contract_fixture, validate_voice_case_result_against_contract,
    VoiceContractCase, VoiceContractFixture, VoiceReplayResult, VoiceReplayStep,
};
use tau_core::{
    append_line_with_rotation, current_unix_timestamp_ms, write_text_atomic, LogRotationPolicy,
};
use tau_runtime::channel_store::{ChannelContextEntry, ChannelLogEntry, ChannelStore};
use tau_runtime::transport_health::TransportHealthSnapshot;

const VOICE_RUNTIME_STATE_SCHEMA_VERSION: u32 = 1;
const VOICE_LIVE_INPUT_SCHEMA_VERSION: u32 = 1;
const VOICE_RUNTIME_EVENTS_LOG_FILE: &str = "runtime-events.jsonl";

fn voice_runtime_state_schema_version() -> u32 {
    VOICE_RUNTIME_STATE_SCHEMA_VERSION
}

fn voice_live_input_schema_version() -> u32 {
    VOICE_LIVE_INPUT_SCHEMA_VERSION
}

#[derive(Debug, Clone)]
/// Public struct `VoiceRuntimeConfig` used across Tau components.
pub struct VoiceRuntimeConfig {
    pub fixture_path: PathBuf,
    pub state_dir: PathBuf,
    pub queue_limit: usize,
    pub processed_case_cap: usize,
    pub retry_max_attempts: usize,
    pub retry_base_delay_ms: u64,
}

#[derive(Debug, Clone)]
/// Public struct `VoiceLiveRuntimeConfig` used across Tau components.
pub struct VoiceLiveRuntimeConfig {
    pub input_path: PathBuf,
    pub state_dir: PathBuf,
    pub wake_word: String,
    pub max_turns: usize,
    pub tts_output_enabled: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
/// Public struct `VoiceRuntimeSummary` used across Tau components.
pub struct VoiceRuntimeSummary {
    pub discovered_cases: usize,
    pub queued_cases: usize,
    pub applied_cases: usize,
    pub duplicate_skips: usize,
    pub malformed_cases: usize,
    pub retryable_failures: usize,
    pub retry_attempts: usize,
    pub failed_cases: usize,
    pub wake_word_detections: usize,
    pub handled_turns: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
/// Public struct `VoiceLiveRuntimeSummary` used across Tau components.
pub struct VoiceLiveRuntimeSummary {
    pub discovered_frames: usize,
    pub queued_frames: usize,
    pub wake_word_detections: usize,
    pub handled_turns: usize,
    pub ignored_frames: usize,
    pub invalid_audio_frames: usize,
    pub provider_outages: usize,
    pub tts_outputs: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum VoiceLiveSessionState {
    Idle,
    Listening,
    Processing,
    Responding,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VoiceLiveInputFixture {
    #[serde(default = "voice_live_input_schema_version")]
    schema_version: u32,
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    frames: Vec<VoiceLiveFrame>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VoiceLiveFrame {
    #[serde(default)]
    frame_id: String,
    #[serde(default)]
    transcript: String,
    #[serde(default)]
    speaker_id: String,
    #[serde(default = "default_voice_locale")]
    locale: String,
    #[serde(default)]
    invalid_audio: bool,
    #[serde(default)]
    provider_outage: bool,
}

#[derive(Debug, Clone, Serialize)]
struct VoiceRuntimeCycleReport {
    timestamp_unix_ms: u64,
    health_state: String,
    health_reason: String,
    reason_codes: Vec<String>,
    discovered_cases: usize,
    queued_cases: usize,
    applied_cases: usize,
    duplicate_skips: usize,
    malformed_cases: usize,
    retryable_failures: usize,
    retry_attempts: usize,
    failed_cases: usize,
    wake_word_detections: usize,
    handled_turns: usize,
    backlog_cases: usize,
    failure_streak: usize,
}

#[derive(Debug, Clone, Serialize)]
struct VoiceLiveCycleReport {
    timestamp_unix_ms: u64,
    health_state: String,
    health_reason: String,
    reason_codes: Vec<String>,
    session_id: String,
    session_state: String,
    wake_word: String,
    discovered_frames: usize,
    queued_frames: usize,
    wake_word_detections: usize,
    handled_turns: usize,
    ignored_frames: usize,
    invalid_audio_frames: usize,
    provider_outages: usize,
    tts_outputs: usize,
    backlog_frames: usize,
    failure_streak: usize,
}

struct VoiceLiveReportMetadata<'a> {
    session_id: &'a str,
    session_state: VoiceLiveSessionState,
    wake_word: &'a str,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct VoiceInteractionRecord {
    case_key: String,
    case_id: String,
    mode: String,
    wake_word: String,
    locale: String,
    speaker_id: String,
    utterance: String,
    last_status_code: u16,
    last_outcome: String,
    run_count: u64,
    updated_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VoiceRuntimeState {
    #[serde(default = "voice_runtime_state_schema_version")]
    schema_version: u32,
    #[serde(default)]
    processed_case_keys: Vec<String>,
    #[serde(default)]
    interactions: Vec<VoiceInteractionRecord>,
    #[serde(default)]
    health: TransportHealthSnapshot,
}

impl Default for VoiceRuntimeState {
    fn default() -> Self {
        Self {
            schema_version: VOICE_RUNTIME_STATE_SCHEMA_VERSION,
            processed_case_keys: Vec::new(),
            interactions: Vec::new(),
            health: TransportHealthSnapshot::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct VoiceMutationCounts {
    wake_word_detections: usize,
    handled_turns: usize,
}

pub async fn run_voice_contract_runner(config: VoiceRuntimeConfig) -> Result<()> {
    let fixture = load_voice_contract_fixture(&config.fixture_path)?;
    let mut runtime = VoiceRuntime::new(config)?;
    let summary = runtime.run_once(&fixture).await?;
    let health = runtime.transport_health().clone();
    let classification = health.classify();

    println!(
        "voice runner summary: discovered={} queued={} applied={} duplicate_skips={} malformed={} retryable_failures={} retries={} failed={} wake_word_detections={} handled_turns={}",
        summary.discovered_cases,
        summary.queued_cases,
        summary.applied_cases,
        summary.duplicate_skips,
        summary.malformed_cases,
        summary.retryable_failures,
        summary.retry_attempts,
        summary.failed_cases,
        summary.wake_word_detections,
        summary.handled_turns
    );
    println!(
        "voice runner health: state={} failure_streak={} queue_depth={} reason={}",
        classification.state.as_str(),
        health.failure_streak,
        health.queue_depth,
        classification.reason
    );

    Ok(())
}

pub async fn run_voice_live_runner(config: VoiceLiveRuntimeConfig) -> Result<()> {
    let fixture = load_voice_live_input_fixture(&config.input_path)?;
    let mut runtime = VoiceLiveRuntime::new(config)?;
    let summary = runtime.run_once(&fixture).await?;
    let health = runtime.transport_health().clone();
    let classification = health.classify();

    println!(
        "voice live runner summary: discovered={} queued={} wake_word_detections={} handled_turns={} ignored={} invalid_audio={} provider_outages={} tts_outputs={}",
        summary.discovered_frames,
        summary.queued_frames,
        summary.wake_word_detections,
        summary.handled_turns,
        summary.ignored_frames,
        summary.invalid_audio_frames,
        summary.provider_outages,
        summary.tts_outputs
    );
    println!(
        "voice live runner health: state={} failure_streak={} queue_depth={} reason={}",
        classification.state.as_str(),
        health.failure_streak,
        health.queue_depth,
        classification.reason
    );

    Ok(())
}

struct VoiceLiveRuntime {
    config: VoiceLiveRuntimeConfig,
    state: VoiceRuntimeState,
    session_state: VoiceLiveSessionState,
}

impl VoiceLiveRuntime {
    fn new(config: VoiceLiveRuntimeConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.state_dir)
            .with_context(|| format!("failed to create {}", config.state_dir.display()))?;
        let mut state = load_voice_runtime_state(&config.state_dir.join("state.json"))?;
        state
            .interactions
            .sort_by(|left, right| left.speaker_id.cmp(&right.speaker_id));
        Ok(Self {
            config,
            state,
            session_state: VoiceLiveSessionState::Idle,
        })
    }

    fn state_path(&self) -> PathBuf {
        self.config.state_dir.join("state.json")
    }

    fn transport_health(&self) -> &TransportHealthSnapshot {
        &self.state.health
    }

    fn transition(&mut self, next_state: VoiceLiveSessionState) {
        self.session_state = next_state;
    }

    async fn run_once(
        &mut self,
        fixture: &VoiceLiveInputFixture,
    ) -> Result<VoiceLiveRuntimeSummary> {
        let cycle_started = Instant::now();
        let mut summary = VoiceLiveRuntimeSummary {
            discovered_frames: fixture.frames.len(),
            ..VoiceLiveRuntimeSummary::default()
        };
        let mut buffered_frames = fixture.frames.clone();
        buffered_frames.truncate(self.config.max_turns.max(1));
        summary.queued_frames = buffered_frames.len();

        let session_id = normalized_live_session_id(&fixture.session_id);
        let wake_word = self.config.wake_word.trim().to_ascii_lowercase();

        for (index, frame) in buffered_frames.iter().enumerate() {
            let frame_id = normalized_live_frame_id(frame, index);
            let frame_key = live_frame_runtime_key(&session_id, frame, &frame_id);
            self.transition(VoiceLiveSessionState::Listening);

            if frame.invalid_audio || frame.transcript.trim().is_empty() {
                summary.invalid_audio_frames = summary.invalid_audio_frames.saturating_add(1);
                self.persist_live_non_success_result(
                    &frame_key,
                    frame,
                    "invalid_audio",
                    422,
                    "invalid_audio_frame",
                    false,
                )?;
                self.transition(VoiceLiveSessionState::Idle);
                continue;
            }

            let utterance = extract_live_utterance(&wake_word, frame.transcript.as_str());
            if utterance.is_none() {
                summary.ignored_frames = summary.ignored_frames.saturating_add(1);
                self.persist_live_non_success_result(
                    &frame_key,
                    frame,
                    "ignored_no_wake_word",
                    204,
                    "",
                    false,
                )?;
                self.transition(VoiceLiveSessionState::Idle);
                continue;
            }

            summary.wake_word_detections = summary.wake_word_detections.saturating_add(1);
            let utterance = utterance.unwrap_or_default();
            if frame.provider_outage {
                self.transition(VoiceLiveSessionState::Processing);
                summary.provider_outages = summary.provider_outages.saturating_add(1);
                self.persist_live_non_success_result(
                    &frame_key,
                    frame,
                    "provider_outage",
                    503,
                    "voice_backend_unavailable",
                    true,
                )?;
                self.transition(VoiceLiveSessionState::Idle);
                continue;
            }

            if utterance.is_empty() {
                self.persist_live_non_success_result(
                    &frame_key,
                    frame,
                    "wake_word_only",
                    202,
                    "",
                    true,
                )?;
                self.transition(VoiceLiveSessionState::Idle);
                continue;
            }

            self.transition(VoiceLiveSessionState::Processing);
            let response_text = format!("acknowledged: {utterance}");
            self.transition(VoiceLiveSessionState::Responding);
            if self.persist_live_success_result(
                &frame_key,
                &frame_id,
                frame,
                utterance.as_str(),
                response_text.as_str(),
            )? {
                summary.tts_outputs = summary.tts_outputs.saturating_add(1);
            }
            summary.handled_turns = summary.handled_turns.saturating_add(1);
            self.transition(VoiceLiveSessionState::Idle);
        }

        let cycle_duration_ms =
            u64::try_from(cycle_started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let health = build_voice_live_transport_health_snapshot(
            &summary,
            cycle_duration_ms,
            self.state.health.failure_streak,
        );
        let classification = health.classify();
        let reason_codes = cycle_reason_codes_live(&summary);
        self.state.health = health.clone();
        let report_metadata = VoiceLiveReportMetadata {
            session_id: &session_id,
            session_state: self.session_state,
            wake_word: &wake_word,
        };

        save_voice_runtime_state(&self.state_path(), &self.state)?;
        append_voice_live_cycle_report(
            &self.config.state_dir.join(VOICE_RUNTIME_EVENTS_LOG_FILE),
            &summary,
            &health,
            &classification.reason,
            &reason_codes,
            &report_metadata,
        )?;
        Ok(summary)
    }

    fn persist_live_success_result(
        &mut self,
        frame_key: &str,
        frame_id: &str,
        frame: &VoiceLiveFrame,
        utterance: &str,
        response_text: &str,
    ) -> Result<bool> {
        let speaker_id = normalize_live_speaker_id(frame.speaker_id.as_str());
        let locale = normalize_live_locale(frame.locale.as_str());
        let timestamp_unix_ms = current_unix_timestamp_ms();
        let wake_word = self.config.wake_word.trim().to_ascii_lowercase();
        let run_count = self
            .state
            .interactions
            .iter()
            .find(|existing| existing.case_key == frame_key)
            .map_or(0, |existing| existing.run_count)
            .saturating_add(1);

        let record = VoiceInteractionRecord {
            case_key: frame_key.to_string(),
            case_id: frame_id.to_string(),
            mode: "live_turn".to_string(),
            wake_word: wake_word.clone(),
            locale: locale.clone(),
            speaker_id: speaker_id.clone(),
            utterance: utterance.to_string(),
            last_status_code: 202,
            last_outcome: "success".to_string(),
            run_count,
            updated_unix_ms: timestamp_unix_ms,
        };

        if let Some(existing) = self
            .state
            .interactions
            .iter_mut()
            .find(|existing| existing.case_key == frame_key)
        {
            *existing = record;
        } else {
            self.state.interactions.push(record);
        }
        self.state
            .interactions
            .sort_by(|left, right| left.speaker_id.cmp(&right.speaker_id));

        let store = ChannelStore::open(
            &self.config.state_dir.join("channel-store"),
            "voice",
            &speaker_id,
        )?;
        store.append_log_entry(&ChannelLogEntry {
            timestamp_unix_ms,
            direction: "system".to_string(),
            event_key: Some(frame_key.to_string()),
            source: "tau-voice-live-runner".to_string(),
            payload: json!({
                "outcome":"success",
                "mode":"live_turn",
                "frame_id": frame_id,
                "speaker_id": speaker_id,
                "wake_word": wake_word,
                "locale": locale,
                "utterance": utterance,
                "response_text": response_text,
                "status_code": 202,
            }),
        })?;
        store.append_context_entry(&ChannelContextEntry {
            timestamp_unix_ms,
            role: "system".to_string(),
            text: format!(
                "voice live frame {} handled speaker={} utterance={}",
                frame_id,
                normalize_live_speaker_id(frame.speaker_id.as_str()),
                utterance
            ),
        })?;

        let mut tts_written = false;
        if self.config.tts_output_enabled {
            store.append_log_entry(&ChannelLogEntry {
                timestamp_unix_ms,
                direction: "assistant".to_string(),
                event_key: Some(frame_key.to_string()),
                source: "tau-voice-live-runner".to_string(),
                payload: json!({
                    "outcome":"tts_output",
                    "text": response_text,
                    "voice_id":"default",
                    "mime_type":"audio/wav",
                }),
            })?;
            tts_written = true;
        }

        store.write_memory(&render_voice_snapshot(
            &self.state.interactions,
            &speaker_id,
        ))?;
        Ok(tts_written)
    }

    fn persist_live_non_success_result(
        &self,
        frame_key: &str,
        frame: &VoiceLiveFrame,
        outcome: &str,
        status_code: u16,
        error_code: &str,
        wake_word_detected: bool,
    ) -> Result<()> {
        let speaker_id = normalize_live_speaker_id(frame.speaker_id.as_str());
        let locale = normalize_live_locale(frame.locale.as_str());
        let timestamp_unix_ms = current_unix_timestamp_ms();
        let store = ChannelStore::open(
            &self.config.state_dir.join("channel-store"),
            "voice",
            &speaker_id,
        )?;
        store.append_log_entry(&ChannelLogEntry {
            timestamp_unix_ms,
            direction: "system".to_string(),
            event_key: Some(frame_key.to_string()),
            source: "tau-voice-live-runner".to_string(),
            payload: json!({
                "outcome": outcome,
                "frame_id": frame.frame_id,
                "speaker_id": speaker_id,
                "wake_word": self.config.wake_word.trim().to_ascii_lowercase(),
                "wake_word_detected": wake_word_detected,
                "locale": locale,
                "transcript": frame.transcript.trim(),
                "status_code": status_code,
                "error_code": error_code,
            }),
        })?;
        store.append_context_entry(&ChannelContextEntry {
            timestamp_unix_ms,
            role: "system".to_string(),
            text: format!(
                "voice live frame {} outcome={} error_code={} status={}",
                frame.frame_id, outcome, error_code, status_code
            ),
        })?;
        Ok(())
    }
}

struct VoiceRuntime {
    config: VoiceRuntimeConfig,
    state: VoiceRuntimeState,
    processed_case_keys: HashSet<String>,
}

impl VoiceRuntime {
    fn new(config: VoiceRuntimeConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.state_dir)
            .with_context(|| format!("failed to create {}", config.state_dir.display()))?;
        let mut state = load_voice_runtime_state(&config.state_dir.join("state.json"))?;
        state.processed_case_keys =
            normalize_processed_case_keys(&state.processed_case_keys, config.processed_case_cap);
        state
            .interactions
            .sort_by(|left, right| left.speaker_id.cmp(&right.speaker_id));
        let processed_case_keys = state.processed_case_keys.iter().cloned().collect();
        Ok(Self {
            config,
            state,
            processed_case_keys,
        })
    }

    fn state_path(&self) -> PathBuf {
        self.config.state_dir.join("state.json")
    }

    fn transport_health(&self) -> &TransportHealthSnapshot {
        &self.state.health
    }

    async fn run_once(&mut self, fixture: &VoiceContractFixture) -> Result<VoiceRuntimeSummary> {
        let cycle_started = Instant::now();
        let mut summary = VoiceRuntimeSummary {
            discovered_cases: fixture.cases.len(),
            ..VoiceRuntimeSummary::default()
        };

        let mut queued_cases = fixture.cases.clone();
        queued_cases.truncate(self.config.queue_limit);
        summary.queued_cases = queued_cases.len();

        for case in queued_cases {
            let case_key = case_runtime_key(&case);
            if self.processed_case_keys.contains(&case_key) {
                summary.duplicate_skips = summary.duplicate_skips.saturating_add(1);
                continue;
            }

            let mut attempt = 1usize;
            loop {
                let result = evaluate_voice_case(&case);
                validate_voice_case_result_against_contract(&case, &result)?;

                match result.step {
                    VoiceReplayStep::Success => {
                        let mutation = self.persist_success_result(&case, &case_key, &result)?;
                        summary.applied_cases = summary.applied_cases.saturating_add(1);
                        summary.wake_word_detections = summary
                            .wake_word_detections
                            .saturating_add(mutation.wake_word_detections);
                        summary.handled_turns =
                            summary.handled_turns.saturating_add(mutation.handled_turns);
                        self.record_processed_case(&case_key);
                        break;
                    }
                    VoiceReplayStep::MalformedInput => {
                        summary.malformed_cases = summary.malformed_cases.saturating_add(1);
                        self.persist_non_success_result(&case, &case_key, &result)?;
                        self.record_processed_case(&case_key);
                        break;
                    }
                    VoiceReplayStep::RetryableFailure => {
                        summary.retryable_failures = summary.retryable_failures.saturating_add(1);
                        if attempt >= self.config.retry_max_attempts {
                            summary.failed_cases = summary.failed_cases.saturating_add(1);
                            self.persist_non_success_result(&case, &case_key, &result)?;
                            break;
                        }
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

        save_voice_runtime_state(&self.state_path(), &self.state)?;
        append_voice_cycle_report(
            &self.config.state_dir.join(VOICE_RUNTIME_EVENTS_LOG_FILE),
            &summary,
            &health,
            &classification.reason,
            &reason_codes,
        )?;

        Ok(summary)
    }

    fn persist_success_result(
        &mut self,
        case: &VoiceContractCase,
        case_key: &str,
        result: &VoiceReplayResult,
    ) -> Result<VoiceMutationCounts> {
        let mode = case.mode.as_str().to_string();
        let wake_word = case.wake_word.trim().to_ascii_lowercase();
        let locale = case.locale.trim().to_string();
        let speaker_id = normalized_speaker_id(case);
        let timestamp_unix_ms = current_unix_timestamp_ms();
        let utterance = result
            .response_body
            .get("utterance")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();

        let run_count = self
            .state
            .interactions
            .iter()
            .find(|existing| existing.case_key == case_key)
            .map_or(0, |existing| existing.run_count)
            .saturating_add(1);

        let record = VoiceInteractionRecord {
            case_key: case_key.to_string(),
            case_id: case.case_id.clone(),
            mode: mode.clone(),
            wake_word: wake_word.clone(),
            locale: locale.clone(),
            speaker_id: speaker_id.clone(),
            utterance: utterance.clone(),
            last_status_code: result.status_code,
            last_outcome: "success".to_string(),
            run_count,
            updated_unix_ms: timestamp_unix_ms,
        };

        if let Some(existing) = self
            .state
            .interactions
            .iter_mut()
            .find(|existing| existing.case_key == case_key)
        {
            *existing = record;
        } else {
            self.state.interactions.push(record);
        }
        self.state
            .interactions
            .sort_by(|left, right| left.speaker_id.cmp(&right.speaker_id));

        let mutation = if mode == "wake_word" {
            VoiceMutationCounts {
                wake_word_detections: 1,
                handled_turns: 0,
            }
        } else {
            VoiceMutationCounts {
                wake_word_detections: 0,
                handled_turns: 1,
            }
        };

        if let Some(store) = self.scope_channel_store(case)? {
            store.append_log_entry(&ChannelLogEntry {
                timestamp_unix_ms,
                direction: "system".to_string(),
                event_key: Some(case_key.to_string()),
                source: "tau-voice-runner".to_string(),
                payload: json!({
                    "outcome":"success",
                    "case_id": case.case_id,
                    "mode": mode,
                    "speaker_id": speaker_id,
                    "wake_word": wake_word,
                    "locale": locale,
                    "utterance": utterance,
                    "status_code": result.status_code,
                }),
            })?;
            store.append_context_entry(&ChannelContextEntry {
                timestamp_unix_ms,
                role: "system".to_string(),
                text: format!(
                    "voice case {} applied mode={} speaker={} status={}",
                    case.case_id,
                    case.mode.as_str(),
                    normalized_speaker_id(case),
                    result.status_code
                ),
            })?;
            store.write_memory(&render_voice_snapshot(
                &self.state.interactions,
                &channel_id_for_case(case),
            ))?;
        }
        Ok(mutation)
    }

    fn persist_non_success_result(
        &self,
        case: &VoiceContractCase,
        case_key: &str,
        result: &VoiceReplayResult,
    ) -> Result<()> {
        if let Some(store) = self.scope_channel_store(case)? {
            let timestamp_unix_ms = current_unix_timestamp_ms();
            let outcome = outcome_name(result.step);
            store.append_log_entry(&ChannelLogEntry {
                timestamp_unix_ms,
                direction: "system".to_string(),
                event_key: Some(case_key.to_string()),
                source: "tau-voice-runner".to_string(),
                payload: json!({
                    "outcome": outcome,
                    "case_id": case.case_id,
                    "mode": case.mode.as_str(),
                    "speaker_id": normalized_speaker_id(case),
                    "wake_word": case.wake_word.trim().to_ascii_lowercase(),
                    "status_code": result.status_code,
                    "error_code": result.error_code.clone().unwrap_or_default(),
                }),
            })?;
            store.append_context_entry(&ChannelContextEntry {
                timestamp_unix_ms,
                role: "system".to_string(),
                text: format!(
                    "voice case {} outcome={} error_code={} status={}",
                    case.case_id,
                    outcome,
                    result.error_code.clone().unwrap_or_default(),
                    result.status_code
                ),
            })?;
        }
        Ok(())
    }

    fn scope_channel_store(&self, case: &VoiceContractCase) -> Result<Option<ChannelStore>> {
        let channel_id = channel_id_for_case(case);
        let store = ChannelStore::open(
            &self.config.state_dir.join("channel-store"),
            "voice",
            &channel_id,
        )?;
        Ok(Some(store))
    }

    fn record_processed_case(&mut self, case_key: &str) {
        if self.processed_case_keys.contains(case_key) {
            return;
        }
        self.state.processed_case_keys.push(case_key.to_string());
        self.processed_case_keys.insert(case_key.to_string());
        if self.state.processed_case_keys.len() > self.config.processed_case_cap {
            let overflow = self
                .state
                .processed_case_keys
                .len()
                .saturating_sub(self.config.processed_case_cap);
            let removed = self.state.processed_case_keys.drain(0..overflow);
            for key in removed {
                self.processed_case_keys.remove(&key);
            }
        }
    }
}

fn default_voice_locale() -> String {
    "en-US".to_string()
}

fn normalize_live_locale(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return default_voice_locale();
    }
    trimmed.to_string()
}

fn normalize_live_speaker_id(raw: &str) -> String {
    let speaker_id = raw.trim();
    if speaker_id.is_empty() {
        return "voice".to_string();
    }
    if speaker_id
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_')
    {
        return speaker_id.to_string();
    }
    "voice".to_string()
}

fn normalized_live_session_id(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "voice-live".to_string();
    }
    trimmed.to_string()
}

fn normalized_live_frame_id(frame: &VoiceLiveFrame, index: usize) -> String {
    let trimmed = frame.frame_id.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    format!("frame-{}", index.saturating_add(1))
}

fn live_frame_runtime_key(session_id: &str, frame: &VoiceLiveFrame, frame_id: &str) -> String {
    format!(
        "live:{}:{}:{}",
        session_id.trim(),
        normalize_live_speaker_id(frame.speaker_id.as_str()),
        frame_id
    )
}

fn extract_live_utterance(wake_word: &str, transcript: &str) -> Option<String> {
    let trimmed = transcript.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut parts = trimmed.split_whitespace();
    let first_token = parts.next()?.to_ascii_lowercase();
    let normalized_wake_word = wake_word.trim().to_ascii_lowercase();
    if first_token != normalized_wake_word {
        return None;
    }
    Some(parts.collect::<Vec<_>>().join(" ").trim().to_string())
}

fn load_voice_live_input_fixture(path: &Path) -> Result<VoiceLiveInputFixture> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let mut parsed = serde_json::from_str::<VoiceLiveInputFixture>(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    if parsed.schema_version != VOICE_LIVE_INPUT_SCHEMA_VERSION {
        anyhow::bail!(
            "unsupported voice live schema_version={} in {}",
            parsed.schema_version,
            path.display()
        );
    }
    if parsed.session_id.trim().is_empty() {
        parsed.session_id = "voice-live".to_string();
    }
    for (index, frame) in parsed.frames.iter_mut().enumerate() {
        if frame.frame_id.trim().is_empty() {
            frame.frame_id = format!("frame-{}", index.saturating_add(1));
        }
        frame.locale = normalize_live_locale(frame.locale.as_str());
        frame.speaker_id = normalize_live_speaker_id(frame.speaker_id.as_str());
        frame.transcript = frame.transcript.trim().to_string();
    }
    Ok(parsed)
}

fn normalized_speaker_id(case: &VoiceContractCase) -> String {
    let speaker_id = case.speaker_id.trim();
    if speaker_id.is_empty() {
        return "voice".to_string();
    }
    if speaker_id
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_')
    {
        return speaker_id.to_string();
    }
    "voice".to_string()
}

fn channel_id_for_case(case: &VoiceContractCase) -> String {
    normalized_speaker_id(case)
}

fn case_runtime_key(case: &VoiceContractCase) -> String {
    format!(
        "{}:{}:{}:{}",
        case.mode.as_str(),
        case.wake_word.trim().to_ascii_lowercase(),
        normalized_speaker_id(case),
        case.case_id.trim()
    )
}

fn outcome_name(step: VoiceReplayStep) -> &'static str {
    match step {
        VoiceReplayStep::Success => "success",
        VoiceReplayStep::MalformedInput => "malformed_input",
        VoiceReplayStep::RetryableFailure => "retryable_failure",
    }
}

fn build_transport_health_snapshot(
    summary: &VoiceRuntimeSummary,
    cycle_duration_ms: u64,
    previous_failure_streak: usize,
) -> TransportHealthSnapshot {
    let backlog_cases = summary
        .discovered_cases
        .saturating_sub(summary.queued_cases);
    let failure_streak = if summary.failed_cases > 0 {
        previous_failure_streak.saturating_add(1)
    } else {
        0
    };

    TransportHealthSnapshot {
        updated_unix_ms: current_unix_timestamp_ms(),
        cycle_duration_ms,
        queue_depth: backlog_cases,
        active_runs: 0,
        failure_streak,
        last_cycle_discovered: summary.discovered_cases,
        last_cycle_processed: summary
            .applied_cases
            .saturating_add(summary.malformed_cases)
            .saturating_add(summary.failed_cases)
            .saturating_add(summary.duplicate_skips),
        last_cycle_completed: summary
            .applied_cases
            .saturating_add(summary.malformed_cases),
        last_cycle_failed: summary.failed_cases,
        last_cycle_duplicates: summary.duplicate_skips,
    }
}

fn build_voice_live_transport_health_snapshot(
    summary: &VoiceLiveRuntimeSummary,
    cycle_duration_ms: u64,
    previous_failure_streak: usize,
) -> TransportHealthSnapshot {
    let backlog_frames = summary
        .discovered_frames
        .saturating_sub(summary.queued_frames);
    let failed = summary
        .invalid_audio_frames
        .saturating_add(summary.provider_outages);
    let failure_streak = if failed > 0 {
        previous_failure_streak.saturating_add(1)
    } else {
        0
    };

    TransportHealthSnapshot {
        updated_unix_ms: current_unix_timestamp_ms(),
        cycle_duration_ms,
        queue_depth: backlog_frames,
        active_runs: 0,
        failure_streak,
        last_cycle_discovered: summary.discovered_frames,
        last_cycle_processed: summary
            .handled_turns
            .saturating_add(summary.ignored_frames)
            .saturating_add(summary.invalid_audio_frames)
            .saturating_add(summary.provider_outages),
        last_cycle_completed: summary.handled_turns,
        last_cycle_failed: failed,
        last_cycle_duplicates: 0,
    }
}

fn cycle_reason_codes(summary: &VoiceRuntimeSummary) -> Vec<String> {
    let mut codes = Vec::new();
    if summary.discovered_cases > summary.queued_cases {
        codes.push("queue_backpressure_applied".to_string());
    }
    if summary.duplicate_skips > 0 {
        codes.push("duplicate_cases_skipped".to_string());
    }
    if summary.malformed_cases > 0 {
        codes.push("malformed_inputs_observed".to_string());
    }
    if summary.retry_attempts > 0 {
        codes.push("retry_attempted".to_string());
    }
    if summary.retryable_failures > 0 {
        codes.push("retryable_failures_observed".to_string());
    }
    if summary.failed_cases > 0 {
        codes.push("case_processing_failed".to_string());
    }
    if summary.wake_word_detections > 0 {
        codes.push("wake_word_detected".to_string());
    }
    if summary.handled_turns > 0 {
        codes.push("turns_handled".to_string());
    }
    if codes.is_empty() {
        codes.push("healthy_cycle".to_string());
    }
    codes
}

fn cycle_reason_codes_live(summary: &VoiceLiveRuntimeSummary) -> Vec<String> {
    let mut codes = Vec::new();
    if summary.discovered_frames > summary.queued_frames {
        codes.push("queue_backpressure_applied".to_string());
    }
    if summary.wake_word_detections > 0 {
        codes.push("wake_word_detected".to_string());
    }
    if summary.handled_turns > 0 {
        codes.push("turns_handled".to_string());
    }
    if summary.ignored_frames > 0 {
        codes.push("frames_ignored_no_wake_word".to_string());
    }
    if summary.invalid_audio_frames > 0 {
        codes.push("invalid_audio_frames_observed".to_string());
    }
    if summary.provider_outages > 0 {
        codes.push("provider_outage_observed".to_string());
    }
    if summary.tts_outputs > 0 {
        codes.push("tts_output_emitted".to_string());
    }
    if codes.is_empty() {
        codes.push("healthy_cycle".to_string());
    }
    codes
}

fn append_voice_cycle_report(
    path: &Path,
    summary: &VoiceRuntimeSummary,
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
    let payload = VoiceRuntimeCycleReport {
        timestamp_unix_ms: current_unix_timestamp_ms(),
        health_state: health.classify().state.as_str().to_string(),
        health_reason: health_reason.to_string(),
        reason_codes: reason_codes.to_vec(),
        discovered_cases: summary.discovered_cases,
        queued_cases: summary.queued_cases,
        applied_cases: summary.applied_cases,
        duplicate_skips: summary.duplicate_skips,
        malformed_cases: summary.malformed_cases,
        retryable_failures: summary.retryable_failures,
        retry_attempts: summary.retry_attempts,
        failed_cases: summary.failed_cases,
        wake_word_detections: summary.wake_word_detections,
        handled_turns: summary.handled_turns,
        backlog_cases: summary
            .discovered_cases
            .saturating_sub(summary.queued_cases),
        failure_streak: health.failure_streak,
    };
    let line = serde_json::to_string(&payload).context("serialize voice runtime report")?;
    append_line_with_rotation(path, &line, LogRotationPolicy::from_env())
        .with_context(|| format!("failed to append {}", path.display()))?;
    Ok(())
}

fn append_voice_live_cycle_report(
    path: &Path,
    summary: &VoiceLiveRuntimeSummary,
    health: &TransportHealthSnapshot,
    health_reason: &str,
    reason_codes: &[String],
    metadata: &VoiceLiveReportMetadata<'_>,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let payload = VoiceLiveCycleReport {
        timestamp_unix_ms: current_unix_timestamp_ms(),
        health_state: health.classify().state.as_str().to_string(),
        health_reason: health_reason.to_string(),
        reason_codes: reason_codes.to_vec(),
        session_id: metadata.session_id.to_string(),
        session_state: match metadata.session_state {
            VoiceLiveSessionState::Idle => "idle",
            VoiceLiveSessionState::Listening => "listening",
            VoiceLiveSessionState::Processing => "processing",
            VoiceLiveSessionState::Responding => "responding",
        }
        .to_string(),
        wake_word: metadata.wake_word.to_string(),
        discovered_frames: summary.discovered_frames,
        queued_frames: summary.queued_frames,
        wake_word_detections: summary.wake_word_detections,
        handled_turns: summary.handled_turns,
        ignored_frames: summary.ignored_frames,
        invalid_audio_frames: summary.invalid_audio_frames,
        provider_outages: summary.provider_outages,
        tts_outputs: summary.tts_outputs,
        backlog_frames: summary
            .discovered_frames
            .saturating_sub(summary.queued_frames),
        failure_streak: health.failure_streak,
    };
    let line = serde_json::to_string(&payload).context("serialize voice live runtime report")?;
    append_line_with_rotation(path, &line, LogRotationPolicy::from_env())
        .with_context(|| format!("failed to append {}", path.display()))?;
    Ok(())
}

fn render_voice_snapshot(records: &[VoiceInteractionRecord], channel_id: &str) -> String {
    let filtered = if channel_id == "voice" {
        records.iter().collect::<Vec<_>>()
    } else {
        records
            .iter()
            .filter(|record| record.speaker_id == channel_id)
            .collect::<Vec<_>>()
    };

    if filtered.is_empty() {
        return format!("# Tau Voice Snapshot ({channel_id})\n\n- No voice interactions");
    }

    let mut lines = vec![
        format!("# Tau Voice Snapshot ({channel_id})"),
        String::new(),
    ];
    for record in filtered {
        lines.push(format!(
            "- speaker={} mode={} wake_word={} status={} utterance={}",
            record.speaker_id,
            record.mode,
            record.wake_word,
            record.last_status_code,
            if record.utterance.is_empty() {
                "-".to_string()
            } else {
                record.utterance.clone()
            }
        ));
    }
    lines.join("\n")
}

fn normalize_processed_case_keys(raw: &[String], cap: usize) -> Vec<String> {
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

fn load_voice_runtime_state(path: &Path) -> Result<VoiceRuntimeState> {
    if !path.exists() {
        return Ok(VoiceRuntimeState::default());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let parsed = match serde_json::from_str::<VoiceRuntimeState>(&raw) {
        Ok(state) => state,
        Err(error) => {
            eprintln!(
                "voice runner: failed to parse state file {} ({error}); starting fresh",
                path.display()
            );
            return Ok(VoiceRuntimeState::default());
        }
    };
    if parsed.schema_version != VOICE_RUNTIME_STATE_SCHEMA_VERSION {
        eprintln!(
            "voice runner: unsupported state schema {} in {}; starting fresh",
            parsed.schema_version,
            path.display()
        );
        return Ok(VoiceRuntimeState::default());
    }
    Ok(parsed)
}

fn save_voice_runtime_state(path: &Path, state: &VoiceRuntimeState) -> Result<()> {
    let payload = serde_json::to_string_pretty(state).context("serialize voice state")?;
    write_text_atomic(path, &payload).with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use serde_json::json;
    use tempfile::tempdir;

    use super::{
        load_voice_live_input_fixture, load_voice_runtime_state, retry_delay_ms,
        run_voice_live_runner, VoiceLiveRuntime, VoiceLiveRuntimeConfig, VoiceRuntime,
        VoiceRuntimeConfig, VOICE_RUNTIME_EVENTS_LOG_FILE,
    };
    use crate::voice_contract::{load_voice_contract_fixture, parse_voice_contract_fixture};
    use tau_runtime::channel_store::ChannelStore;
    use tau_runtime::transport_health::TransportHealthState;

    fn fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("voice-contract")
            .join(name)
    }

    fn live_fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("voice-live")
            .join(name)
    }

    fn build_config(root: &Path) -> VoiceRuntimeConfig {
        VoiceRuntimeConfig {
            fixture_path: fixture_path("mixed-outcomes.json"),
            state_dir: root.join(".tau/voice"),
            queue_limit: 64,
            processed_case_cap: 10_000,
            retry_max_attempts: 2,
            retry_base_delay_ms: 0,
        }
    }

    fn build_live_config(root: &Path, fixture_name: &str) -> VoiceLiveRuntimeConfig {
        VoiceLiveRuntimeConfig {
            input_path: live_fixture_path(fixture_name),
            state_dir: root.join(".tau/voice-live"),
            wake_word: "tau".to_string(),
            max_turns: 64,
            tts_output_enabled: true,
        }
    }

    #[test]
    fn unit_retry_delay_ms_scales_with_attempt_number() {
        assert_eq!(retry_delay_ms(0, 1), 0);
        assert_eq!(retry_delay_ms(10, 1), 10);
        assert_eq!(retry_delay_ms(10, 2), 20);
        assert_eq!(retry_delay_ms(10, 3), 40);
    }

    #[tokio::test]
    async fn functional_runner_processes_fixture_and_persists_voice_snapshot() {
        let temp = tempdir().expect("tempdir");
        let config = build_config(temp.path());
        let fixture =
            load_voice_contract_fixture(&config.fixture_path).expect("fixture should load");
        let mut runtime = VoiceRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");

        assert_eq!(summary.discovered_cases, 3);
        assert_eq!(summary.queued_cases, 3);
        assert_eq!(summary.applied_cases, 1);
        assert_eq!(summary.malformed_cases, 1);
        assert_eq!(summary.retryable_failures, 2);
        assert_eq!(summary.retry_attempts, 1);
        assert_eq!(summary.failed_cases, 1);
        assert_eq!(summary.wake_word_detections, 0);
        assert_eq!(summary.handled_turns, 1);
        assert_eq!(summary.duplicate_skips, 0);

        let state =
            load_voice_runtime_state(&config.state_dir.join("state.json")).expect("load state");
        assert_eq!(state.interactions.len(), 1);
        assert_eq!(state.processed_case_keys.len(), 2);
        assert_eq!(state.health.last_cycle_discovered, 3);
        assert_eq!(state.health.last_cycle_failed, 1);
        assert_eq!(state.health.failure_streak, 1);
        assert_eq!(
            state.health.classify().state,
            TransportHealthState::Degraded
        );

        let events_log =
            std::fs::read_to_string(config.state_dir.join(VOICE_RUNTIME_EVENTS_LOG_FILE))
                .expect("read runtime events");
        assert!(events_log.contains("retryable_failures_observed"));
        assert!(events_log.contains("case_processing_failed"));
        assert!(events_log.contains("turns_handled"));

        let store = ChannelStore::open(&config.state_dir.join("channel-store"), "voice", "ops-1")
            .expect("open channel store");
        let memory = store
            .load_memory()
            .expect("load memory")
            .expect("memory should exist");
        assert!(memory.contains("Tau Voice Snapshot (ops-1)"));
        assert!(memory.contains("open dashboard"));
    }

    #[tokio::test]
    async fn functional_live_runner_handles_single_turn_and_emits_tts() {
        let temp = tempdir().expect("tempdir");
        let config = build_live_config(temp.path(), "single-turn.json");
        let fixture =
            load_voice_live_input_fixture(&config.input_path).expect("fixture should load");
        let mut runtime = VoiceLiveRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");

        assert_eq!(summary.discovered_frames, 1);
        assert_eq!(summary.queued_frames, 1);
        assert_eq!(summary.wake_word_detections, 1);
        assert_eq!(summary.handled_turns, 1);
        assert_eq!(summary.ignored_frames, 0);
        assert_eq!(summary.invalid_audio_frames, 0);
        assert_eq!(summary.provider_outages, 0);
        assert_eq!(summary.tts_outputs, 1);

        let state =
            load_voice_runtime_state(&config.state_dir.join("state.json")).expect("load state");
        assert_eq!(state.interactions.len(), 1);
        assert_eq!(state.interactions[0].utterance, "open dashboard");
        assert_eq!(state.health.last_cycle_completed, 1);
        assert_eq!(state.health.classify().state, TransportHealthState::Healthy);

        let events_log =
            std::fs::read_to_string(config.state_dir.join(VOICE_RUNTIME_EVENTS_LOG_FILE))
                .expect("read runtime events");
        assert!(events_log.contains("tts_output_emitted"));
        assert!(events_log.contains("wake_word_detected"));

        let store =
            ChannelStore::open(&config.state_dir.join("channel-store"), "voice", "ops-live")
                .expect("open channel store");
        let memory = store
            .load_memory()
            .expect("load memory")
            .expect("memory should exist");
        assert!(memory.contains("open dashboard"));
    }

    #[tokio::test]
    async fn functional_live_runner_handles_multi_turn_with_wake_word_routing() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_live_config(temp.path(), "multi-turn.json");
        config.tts_output_enabled = false;
        let fixture =
            load_voice_live_input_fixture(&config.input_path).expect("fixture should load");
        let mut runtime = VoiceLiveRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");

        assert_eq!(summary.discovered_frames, 3);
        assert_eq!(summary.queued_frames, 3);
        assert_eq!(summary.wake_word_detections, 2);
        assert_eq!(summary.handled_turns, 2);
        assert_eq!(summary.ignored_frames, 1);
        assert_eq!(summary.provider_outages, 0);
        assert_eq!(summary.invalid_audio_frames, 0);
        assert_eq!(summary.tts_outputs, 0);

        let state =
            load_voice_runtime_state(&config.state_dir.join("state.json")).expect("load state");
        assert_eq!(state.interactions.len(), 2);
        assert_eq!(state.health.last_cycle_discovered, 3);
        assert_eq!(state.health.last_cycle_completed, 2);
    }

    #[tokio::test]
    async fn integration_run_voice_live_runner_entrypoint_processes_test_audio_input() {
        let temp = tempdir().expect("tempdir");
        let config = build_live_config(temp.path(), "single-turn.json");
        run_voice_live_runner(config.clone())
            .await
            .expect("live entrypoint should succeed");

        let state =
            load_voice_runtime_state(&config.state_dir.join("state.json")).expect("load state");
        assert_eq!(state.interactions.len(), 1);
        assert_eq!(state.interactions[0].speaker_id, "ops-live");
        assert_eq!(state.interactions[0].last_outcome, "success");

        let events_log =
            std::fs::read_to_string(config.state_dir.join(VOICE_RUNTIME_EVENTS_LOG_FILE))
                .expect("read runtime events");
        assert!(events_log.contains("ops-live-single"));
    }

    #[tokio::test]
    async fn regression_live_runner_handles_provider_outage_and_invalid_audio_fallback() {
        let temp = tempdir().expect("tempdir");
        let config = build_live_config(temp.path(), "fallbacks.json");
        let fixture =
            load_voice_live_input_fixture(&config.input_path).expect("fixture should load");
        let mut runtime = VoiceLiveRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");

        assert_eq!(summary.discovered_frames, 3);
        assert_eq!(summary.queued_frames, 3);
        assert_eq!(summary.wake_word_detections, 2);
        assert_eq!(summary.handled_turns, 1);
        assert_eq!(summary.invalid_audio_frames, 1);
        assert_eq!(summary.provider_outages, 1);
        assert_eq!(summary.tts_outputs, 1);

        let state =
            load_voice_runtime_state(&config.state_dir.join("state.json")).expect("load state");
        assert_eq!(state.interactions.len(), 1);
        assert_eq!(state.health.last_cycle_failed, 2);
        assert_eq!(state.health.failure_streak, 1);
        assert_eq!(
            state.health.classify().state,
            TransportHealthState::Degraded
        );

        let events_log =
            std::fs::read_to_string(config.state_dir.join(VOICE_RUNTIME_EVENTS_LOG_FILE))
                .expect("read runtime events");
        assert!(events_log.contains("provider_outage_observed"));
        assert!(events_log.contains("invalid_audio_frames_observed"));
    }

    #[tokio::test]
    async fn integration_runner_respects_queue_limit_for_backpressure() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.queue_limit = 2;
        let fixture =
            load_voice_contract_fixture(&config.fixture_path).expect("fixture should load");
        let mut runtime = VoiceRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");

        assert_eq!(summary.discovered_cases, 3);
        assert_eq!(summary.queued_cases, 2);
        assert_eq!(summary.applied_cases, 1);
        assert_eq!(summary.malformed_cases, 1);
        assert_eq!(summary.failed_cases, 0);
        assert_eq!(summary.retryable_failures, 0);

        let state =
            load_voice_runtime_state(&config.state_dir.join("state.json")).expect("load state");
        assert_eq!(state.interactions.len(), 1);
        assert_eq!(state.health.queue_depth, 1);
        assert_eq!(state.health.classify().state, TransportHealthState::Healthy);
    }

    #[tokio::test]
    async fn integration_runner_skips_processed_cases_but_retries_unresolved_failures() {
        let temp = tempdir().expect("tempdir");
        let config = build_config(temp.path());
        let fixture =
            load_voice_contract_fixture(&config.fixture_path).expect("fixture should load");

        let mut first_runtime = VoiceRuntime::new(config.clone()).expect("first runtime");
        let first = first_runtime.run_once(&fixture).await.expect("first run");
        assert_eq!(first.applied_cases, 1);
        assert_eq!(first.malformed_cases, 1);
        assert_eq!(first.failed_cases, 1);

        let mut second_runtime = VoiceRuntime::new(config).expect("second runtime");
        let second = second_runtime.run_once(&fixture).await.expect("second run");
        assert_eq!(second.duplicate_skips, 2);
        assert_eq!(second.applied_cases, 0);
        assert_eq!(second.malformed_cases, 0);
        assert_eq!(second.failed_cases, 1);
    }

    #[tokio::test]
    async fn regression_runner_rejects_contract_drift_between_expected_and_runtime_result() {
        let temp = tempdir().expect("tempdir");
        let mut fixture = load_voice_contract_fixture(&fixture_path("mixed-outcomes.json"))
            .expect("fixture should load");
        let success_case = fixture
            .cases
            .iter_mut()
            .find(|case| case.case_id == "voice-success-turn")
            .expect("success case");
        success_case.expected.response_body = json!({
            "status":"accepted",
            "mode":"turn",
            "wake_word":"tau",
            "utterance":"unexpected",
            "locale":"en-US",
            "speaker_id":"ops-1"
        });

        let fixture_path = temp.path().join("drift-fixture.json");
        std::fs::write(
            &fixture_path,
            serde_json::to_string_pretty(&fixture).expect("serialize"),
        )
        .expect("write fixture");

        let mut config = build_config(temp.path());
        config.fixture_path = fixture_path;

        let mut runtime = VoiceRuntime::new(config).expect("runtime");
        let drift_fixture =
            load_voice_contract_fixture(&runtime.config.fixture_path).expect("fixture should load");
        let error = runtime
            .run_once(&drift_fixture)
            .await
            .expect_err("drift should fail");
        assert!(error.to_string().contains("expected response_body"));
    }

    #[tokio::test]
    async fn regression_runner_failure_streak_resets_after_successful_cycle() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.retry_max_attempts = 1;

        let failing_fixture = parse_voice_contract_fixture(
            r#"{
  "schema_version": 1,
  "name": "retry-only-failure",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "voice-retry-only",
      "mode": "turn",
      "wake_word": "tau",
      "transcript": "tau summarize exposure",
      "locale": "en-US",
      "speaker_id": "ops-r",
      "simulate_retryable_failure": true,
      "expected": {
        "outcome": "retryable_failure",
        "status_code": 503,
        "error_code": "voice_backend_unavailable",
        "response_body": {"status":"retryable","reason":"backend_unavailable"}
      }
    }
  ]
}"#,
        )
        .expect("parse failing fixture");

        let success_fixture = parse_voice_contract_fixture(
            r#"{
  "schema_version": 1,
  "name": "single-success",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "voice-success-only",
      "mode": "turn",
      "wake_word": "tau",
      "transcript": "tau open status board",
      "locale": "en-US",
      "speaker_id": "ops-r",
      "expected": {
        "outcome": "success",
        "status_code": 202,
        "response_body": {
          "status":"accepted",
          "mode":"turn",
          "wake_word":"tau",
          "utterance":"open status board",
          "locale":"en-US",
          "speaker_id":"ops-r"
        }
      }
    }
  ]
}"#,
        )
        .expect("parse success fixture");

        let mut runtime = VoiceRuntime::new(config.clone()).expect("runtime");
        let failed = runtime
            .run_once(&failing_fixture)
            .await
            .expect("failed cycle");
        assert_eq!(failed.failed_cases, 1);
        let state_after_fail = load_voice_runtime_state(&config.state_dir.join("state.json"))
            .expect("load state after fail");
        assert_eq!(state_after_fail.health.failure_streak, 1);

        let success = runtime
            .run_once(&success_fixture)
            .await
            .expect("success cycle");
        assert_eq!(success.failed_cases, 0);
        assert_eq!(success.applied_cases, 1);
        let state_after_success = load_voice_runtime_state(&config.state_dir.join("state.json"))
            .expect("load state after success");
        assert_eq!(state_after_success.health.failure_streak, 0);
        assert_eq!(
            state_after_success.health.classify().state,
            TransportHealthState::Healthy
        );
    }
}
