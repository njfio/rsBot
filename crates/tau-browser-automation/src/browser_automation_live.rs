use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tau_core::current_unix_timestamp_ms;
use tau_runtime::channel_store::{ChannelContextEntry, ChannelLogEntry, ChannelStore};

use crate::browser_automation_contract::{
    assert_browser_automation_result_matches_expectation, BrowserAutomationContractCase,
    BrowserAutomationContractFixture, BrowserAutomationReplayResult, BrowserAutomationReplayStep,
    BROWSER_AUTOMATION_ERROR_ACTION_LIMIT_EXCEEDED, BROWSER_AUTOMATION_ERROR_BACKEND_UNAVAILABLE,
    BROWSER_AUTOMATION_ERROR_TIMEOUT, BROWSER_AUTOMATION_ERROR_UNSAFE_OPERATION,
};

const BROWSER_AUTOMATION_LIVE_EVENTS_LOG_FILE: &str = "runtime-events.jsonl";

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `BrowserAutomationLivePolicy` used across Tau components.
pub struct BrowserAutomationLivePolicy {
    pub action_timeout_ms: u64,
    pub max_actions_per_case: usize,
    pub allow_unsafe_actions: bool,
}

impl Default for BrowserAutomationLivePolicy {
    fn default() -> Self {
        Self {
            action_timeout_ms: 5_000,
            max_actions_per_case: 8,
            allow_unsafe_actions: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `BrowserActionRequest` used across Tau components.
pub struct BrowserActionRequest {
    pub operation: String,
    pub action: String,
    pub url: String,
    pub selector: String,
    pub text: String,
    pub action_repeat_count: usize,
    pub timeout_ms: u64,
}

impl From<&BrowserAutomationContractCase> for BrowserActionRequest {
    fn from(case: &BrowserAutomationContractCase) -> Self {
        Self {
            operation: case.operation.trim().to_ascii_lowercase(),
            action: case.action.trim().to_ascii_lowercase(),
            url: case.url.trim().to_string(),
            selector: case.selector.trim().to_string(),
            text: case.text.trim().to_string(),
            action_repeat_count: case.action_repeat_count,
            timeout_ms: case.timeout_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
/// Public struct `BrowserActionArtifactBundle` used across Tau components.
pub struct BrowserActionArtifactBundle {
    #[serde(default)]
    pub dom_snapshot_html: String,
    #[serde(default)]
    pub screenshot_svg: String,
    #[serde(default)]
    pub trace_json: String,
}

impl BrowserActionArtifactBundle {
    fn is_empty(&self) -> bool {
        self.dom_snapshot_html.trim().is_empty()
            && self.screenshot_svg.trim().is_empty()
            && self.trace_json.trim().is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `BrowserAutomationLivePersistenceConfig` used across Tau components.
pub struct BrowserAutomationLivePersistenceConfig {
    pub state_dir: PathBuf,
    pub artifact_retention_days: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `BrowserActionResult` used across Tau components.
pub struct BrowserActionResult {
    pub status_code: u16,
    #[serde(default)]
    pub error_code: String,
    #[serde(default)]
    pub response_body: serde_json::Value,
    #[serde(default)]
    pub artifacts: BrowserActionArtifactBundle,
}

impl BrowserActionResult {
    fn to_replay_step(&self) -> BrowserAutomationReplayStep {
        if self.status_code >= 500
            || self.error_code == BROWSER_AUTOMATION_ERROR_TIMEOUT
            || self.error_code == BROWSER_AUTOMATION_ERROR_BACKEND_UNAVAILABLE
        {
            return BrowserAutomationReplayStep::RetryableFailure;
        }
        if self.status_code >= 400 {
            return BrowserAutomationReplayStep::MalformedInput;
        }
        BrowserAutomationReplayStep::Success
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `BrowserLiveCaseOutcome` used across Tau components.
pub struct BrowserLiveCaseOutcome {
    pub replay_result: BrowserAutomationReplayResult,
    pub artifacts: BrowserActionArtifactBundle,
}

/// Trait contract for `BrowserActionExecutor` behavior.
pub trait BrowserActionExecutor {
    fn start_session(&mut self) -> Result<()>;
    fn execute_action(&mut self, request: &BrowserActionRequest) -> Result<BrowserActionResult>;
    fn shutdown_session(&mut self) -> Result<()>;
}

#[derive(Debug)]
/// Public struct `PlaywrightCliActionExecutor` used across Tau components.
pub struct PlaywrightCliActionExecutor {
    cli_path: String,
}

impl PlaywrightCliActionExecutor {
    pub fn new(cli_path: impl Into<String>) -> Result<Self> {
        let cli_path = cli_path.into();
        if cli_path.trim().is_empty() {
            bail!("browser automation playwright cli path cannot be empty");
        }
        Ok(Self { cli_path })
    }

    fn invoke_command(
        &self,
        subcommand: &str,
        payload: Option<&BrowserActionRequest>,
    ) -> Result<String> {
        let mut command = Command::new(self.cli_path.trim());
        command.arg(subcommand);
        if let Some(payload) = payload {
            command.arg(
                serde_json::to_string(payload)
                    .context("serialize browser action request payload")?,
            );
        }

        let output = command.output().with_context(|| {
            format!(
                "failed to launch browser automation executor '{}'",
                self.cli_path
            )
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let detail = if !stderr.is_empty() {
                stderr
            } else if !stdout.is_empty() {
                stdout
            } else {
                "no output".to_string()
            };
            bail!("browser executor subcommand '{subcommand}' failed: {detail}");
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

impl BrowserActionExecutor for PlaywrightCliActionExecutor {
    fn start_session(&mut self) -> Result<()> {
        self.invoke_command("start-session", None)?;
        Ok(())
    }

    fn execute_action(&mut self, request: &BrowserActionRequest) -> Result<BrowserActionResult> {
        let output = self.invoke_command("execute-action", Some(request))?;
        if output.trim().is_empty() {
            bail!("browser executor returned empty response for execute-action");
        }
        let parsed = serde_json::from_str::<BrowserActionResult>(&output)
            .with_context(|| format!("failed to parse browser executor response: {output}"))?;
        if parsed.status_code == 0 {
            bail!("browser executor response is missing non-zero status_code");
        }
        Ok(parsed)
    }

    fn shutdown_session(&mut self) -> Result<()> {
        self.invoke_command("shutdown-session", None)?;
        Ok(())
    }
}

#[derive(Debug)]
/// Public struct `BrowserSessionManager` used across Tau components.
pub struct BrowserSessionManager<E: BrowserActionExecutor> {
    executor: E,
    session_started: bool,
    session_shutdown: bool,
}

impl<E: BrowserActionExecutor> BrowserSessionManager<E> {
    pub fn new(executor: E) -> Self {
        Self {
            executor,
            session_started: false,
            session_shutdown: false,
        }
    }

    fn ensure_session_started(&mut self) -> Result<()> {
        if self.session_started {
            return Ok(());
        }
        self.executor.start_session()?;
        self.session_started = true;
        Ok(())
    }

    pub fn execute_case(
        &mut self,
        case: &BrowserAutomationContractCase,
        policy: &BrowserAutomationLivePolicy,
    ) -> Result<BrowserAutomationReplayResult> {
        Ok(self
            .execute_case_with_artifacts(case, policy)?
            .replay_result)
    }

    pub fn execute_case_with_artifacts(
        &mut self,
        case: &BrowserAutomationContractCase,
        policy: &BrowserAutomationLivePolicy,
    ) -> Result<BrowserLiveCaseOutcome> {
        if let Some(policy_result) = enforce_live_policy(case, policy) {
            return Ok(BrowserLiveCaseOutcome {
                replay_result: policy_result,
                artifacts: BrowserActionArtifactBundle::default(),
            });
        }

        self.ensure_session_started()?;
        let request = BrowserActionRequest::from(case);
        let action_result = match self.executor.execute_action(&request) {
            Ok(result) => result,
            Err(error) => BrowserActionResult {
                status_code: 503,
                error_code: BROWSER_AUTOMATION_ERROR_BACKEND_UNAVAILABLE.to_string(),
                response_body: json!({
                    "status": "retryable",
                    "reason": "backend_unavailable",
                    "detail": error.to_string(),
                }),
                artifacts: BrowserActionArtifactBundle::default(),
            },
        };
        let replay_step = action_result.to_replay_step();
        let BrowserActionResult {
            status_code,
            error_code,
            response_body,
            artifacts,
        } = action_result;
        Ok(BrowserLiveCaseOutcome {
            replay_result: BrowserAutomationReplayResult {
                step: replay_step,
                status_code,
                error_code: if error_code.trim().is_empty() {
                    None
                } else {
                    Some(error_code)
                },
                response_body,
            },
            artifacts,
        })
    }

    pub fn shutdown(&mut self) -> Result<()> {
        if !self.session_started || self.session_shutdown {
            return Ok(());
        }
        self.executor.shutdown_session()?;
        self.session_shutdown = true;
        Ok(())
    }
}

impl<E: BrowserActionExecutor> Drop for BrowserSessionManager<E> {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
/// Public struct `BrowserAutomationLiveTimelineEntry` used across Tau components.
pub struct BrowserAutomationLiveTimelineEntry {
    pub case_id: String,
    pub operation: String,
    pub action: String,
    pub replay_step: String,
    pub status_code: u16,
    pub error_code: String,
    #[serde(default)]
    pub artifact_types: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Public struct `BrowserAutomationLiveRunSummary` used across Tau components.
pub struct BrowserAutomationLiveRunSummary {
    pub discovered_cases: usize,
    pub success_cases: usize,
    pub malformed_cases: usize,
    pub retryable_failures: usize,
    pub timeout_failures: usize,
    pub denied_unsafe_actions: usize,
    pub denied_action_limit: usize,
    pub artifact_records: usize,
    #[serde(default)]
    pub timeline: Vec<BrowserAutomationLiveTimelineEntry>,
    #[serde(default)]
    pub reason_codes: Vec<String>,
    #[serde(default)]
    pub health_state: String,
}

pub fn run_browser_automation_live_fixture<E: BrowserActionExecutor>(
    fixture: &BrowserAutomationContractFixture,
    manager: &mut BrowserSessionManager<E>,
    policy: &BrowserAutomationLivePolicy,
) -> Result<BrowserAutomationLiveRunSummary> {
    run_browser_automation_live_fixture_with_persistence(fixture, manager, policy, None)
}

pub fn run_browser_automation_live_fixture_with_persistence<E: BrowserActionExecutor>(
    fixture: &BrowserAutomationContractFixture,
    manager: &mut BrowserSessionManager<E>,
    policy: &BrowserAutomationLivePolicy,
    persistence: Option<&BrowserAutomationLivePersistenceConfig>,
) -> Result<BrowserAutomationLiveRunSummary> {
    let mut summary = BrowserAutomationLiveRunSummary {
        discovered_cases: fixture.cases.len(),
        ..BrowserAutomationLiveRunSummary::default()
    };

    for case in &fixture.cases {
        let outcome = manager.execute_case_with_artifacts(case, policy)?;
        assert_browser_automation_result_matches_expectation(case, &outcome.replay_result)?;
        match outcome.replay_result.step {
            BrowserAutomationReplayStep::Success => {
                summary.success_cases = summary.success_cases.saturating_add(1);
            }
            BrowserAutomationReplayStep::MalformedInput => {
                summary.malformed_cases = summary.malformed_cases.saturating_add(1);
            }
            BrowserAutomationReplayStep::RetryableFailure => {
                summary.retryable_failures = summary.retryable_failures.saturating_add(1);
            }
        }

        if let Some(error_code) = outcome.replay_result.error_code.as_deref() {
            if error_code == BROWSER_AUTOMATION_ERROR_TIMEOUT {
                summary.timeout_failures = summary.timeout_failures.saturating_add(1);
            } else if error_code == BROWSER_AUTOMATION_ERROR_UNSAFE_OPERATION {
                summary.denied_unsafe_actions = summary.denied_unsafe_actions.saturating_add(1);
            } else if error_code == BROWSER_AUTOMATION_ERROR_ACTION_LIMIT_EXCEEDED {
                summary.denied_action_limit = summary.denied_action_limit.saturating_add(1);
            }
        }

        summary.timeline.push(BrowserAutomationLiveTimelineEntry {
            case_id: case.case_id.trim().to_string(),
            operation: case.operation.trim().to_ascii_lowercase(),
            action: case.action.trim().to_ascii_lowercase(),
            replay_step: live_replay_step_label(outcome.replay_result.step).to_string(),
            status_code: outcome.replay_result.status_code,
            error_code: outcome.replay_result.error_code.clone().unwrap_or_default(),
            artifact_types: artifact_types_from_bundle(&outcome.artifacts),
        });

        if let Some(config) = persistence {
            summary.artifact_records = summary
                .artifact_records
                .saturating_add(persist_live_case_artifacts(config, case, &outcome)?);
        }
    }

    summary.reason_codes = live_reason_codes(&summary);
    summary.health_state = live_health_state(&summary).to_string();

    if let Some(config) = persistence {
        append_live_cycle_report(config, &summary)?;
    }

    manager.shutdown()?;
    Ok(summary)
}

fn live_case_key(case: &BrowserAutomationContractCase) -> String {
    format!(
        "{}:{}:{}",
        case.operation.trim().to_ascii_lowercase(),
        case.action.trim().to_ascii_lowercase(),
        case.case_id.trim()
    )
}

fn live_replay_step_label(step: BrowserAutomationReplayStep) -> &'static str {
    match step {
        BrowserAutomationReplayStep::Success => "success",
        BrowserAutomationReplayStep::MalformedInput => "malformed_input",
        BrowserAutomationReplayStep::RetryableFailure => "retryable_failure",
    }
}

fn artifact_types_from_bundle(artifacts: &BrowserActionArtifactBundle) -> Vec<String> {
    let mut types = Vec::new();
    if !artifacts.dom_snapshot_html.trim().is_empty() {
        types.push("dom_snapshot".to_string());
    }
    if !artifacts.screenshot_svg.trim().is_empty() {
        types.push("screenshot".to_string());
    }
    if !artifacts.trace_json.trim().is_empty() {
        types.push("trace".to_string());
    }
    types
}

fn persist_live_case_artifacts(
    config: &BrowserAutomationLivePersistenceConfig,
    case: &BrowserAutomationContractCase,
    outcome: &BrowserLiveCaseOutcome,
) -> Result<usize> {
    let store = ChannelStore::open(
        &config.state_dir.join("channel-store"),
        "browser-automation",
        "live",
    )?;
    let case_key = live_case_key(case);
    let timestamp_unix_ms = current_unix_timestamp_ms();
    let run_id = format!("live-{}", case.case_id.trim());
    let error_code = outcome.replay_result.error_code.clone().unwrap_or_default();

    store.append_log_entry(&ChannelLogEntry {
        timestamp_unix_ms,
        direction: "system".to_string(),
        event_key: Some(case_key.clone()),
        source: "tau-browser-automation-live-runner".to_string(),
        payload: json!({
            "case_id": case.case_id.trim(),
            "operation": case.operation.trim().to_ascii_lowercase(),
            "status_code": outcome.replay_result.status_code,
            "error_code": error_code,
            "response_body": outcome.replay_result.response_body.clone(),
        }),
    })?;
    store.append_context_entry(&ChannelContextEntry {
        timestamp_unix_ms,
        role: "system".to_string(),
        text: format!(
            "browser automation live case {} operation={} status={} error_code={}",
            case.case_id.trim(),
            case.operation.trim().to_ascii_lowercase(),
            outcome.replay_result.status_code,
            outcome
                .replay_result
                .error_code
                .as_deref()
                .unwrap_or_default()
        ),
    })?;

    let mut artifact_records = 0usize;
    if !outcome.artifacts.dom_snapshot_html.trim().is_empty() {
        store.write_text_artifact(
            &run_id,
            "dom_snapshot",
            "private",
            config.artifact_retention_days,
            "html",
            &outcome.artifacts.dom_snapshot_html,
        )?;
        artifact_records = artifact_records.saturating_add(1);
    }
    if !outcome.artifacts.screenshot_svg.trim().is_empty() {
        store.write_text_artifact(
            &run_id,
            "screenshot",
            "private",
            config.artifact_retention_days,
            "svg",
            &outcome.artifacts.screenshot_svg,
        )?;
        artifact_records = artifact_records.saturating_add(1);
    }
    if !outcome.artifacts.trace_json.trim().is_empty() {
        store.write_text_artifact(
            &run_id,
            "trace",
            "private",
            config.artifact_retention_days,
            "json",
            &outcome.artifacts.trace_json,
        )?;
        artifact_records = artifact_records.saturating_add(1);
    }

    if !outcome.artifacts.is_empty() {
        store.write_memory(&render_live_snapshot(case, outcome))?;
    }

    Ok(artifact_records)
}

fn render_live_snapshot(
    case: &BrowserAutomationContractCase,
    outcome: &BrowserLiveCaseOutcome,
) -> String {
    let mut lines = vec![
        "# Tau Browser Automation Live Snapshot".to_string(),
        String::new(),
        format!(
            "- case_id={} operation={} status={} error_code={}",
            case.case_id.trim(),
            case.operation.trim().to_ascii_lowercase(),
            outcome.replay_result.status_code,
            outcome
                .replay_result
                .error_code
                .as_deref()
                .unwrap_or("none"),
        ),
    ];
    if !outcome.artifacts.dom_snapshot_html.trim().is_empty() {
        lines.push("- artifact=dom_snapshot".to_string());
    }
    if !outcome.artifacts.screenshot_svg.trim().is_empty() {
        lines.push("- artifact=screenshot".to_string());
    }
    if !outcome.artifacts.trace_json.trim().is_empty() {
        lines.push("- artifact=trace".to_string());
    }
    lines.join("\n")
}

fn live_reason_codes(summary: &BrowserAutomationLiveRunSummary) -> Vec<String> {
    let mut codes = Vec::new();
    if summary.denied_action_limit > 0 {
        codes.push("action_limit_guardrail_denied".to_string());
    }
    if summary.denied_unsafe_actions > 0 {
        codes.push("unsafe_actions_denied".to_string());
    }
    if summary.timeout_failures > 0 {
        codes.push("timeout_failures_observed".to_string());
    }
    if summary.retryable_failures > 0 {
        codes.push("live_executor_failures_observed".to_string());
    }
    if summary.artifact_records > 0 {
        codes.push("artifact_persisted".to_string());
    }
    if codes.is_empty() {
        codes.push("healthy_cycle".to_string());
    }
    codes
}

fn live_health_state(summary: &BrowserAutomationLiveRunSummary) -> &'static str {
    if summary.retryable_failures > 0 || summary.timeout_failures > 0 {
        "degraded"
    } else {
        "healthy"
    }
}

fn append_live_cycle_report(
    config: &BrowserAutomationLivePersistenceConfig,
    summary: &BrowserAutomationLiveRunSummary,
) -> Result<()> {
    let path = config
        .state_dir
        .join(BROWSER_AUTOMATION_LIVE_EVENTS_LOG_FILE);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }

    let previous_state = load_previous_live_health_state(&path);
    let transition = if previous_state == summary.health_state {
        "steady".to_string()
    } else {
        format!("{}->{}", previous_state, summary.health_state)
    };
    let payload = json!({
        "timestamp_unix_ms": current_unix_timestamp_ms(),
        "health_state": summary.health_state.clone(),
        "previous_health_state": previous_state,
        "health_transition": transition,
        "reason_codes": summary.reason_codes.clone(),
        "discovered_cases": summary.discovered_cases,
        "success_cases": summary.success_cases,
        "malformed_cases": summary.malformed_cases,
        "retryable_failures": summary.retryable_failures,
        "timeout_failures": summary.timeout_failures,
        "denied_unsafe_actions": summary.denied_unsafe_actions,
        "denied_action_limit": summary.denied_action_limit,
        "artifact_records": summary.artifact_records,
    });
    let line = serde_json::to_string(&payload).context("serialize live cycle report")?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    writeln!(file, "{line}").with_context(|| format!("failed to append {}", path.display()))?;
    file.flush()
        .with_context(|| format!("failed to flush {}", path.display()))?;
    Ok(())
}

fn load_previous_live_health_state(path: &Path) -> String {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return "unknown".to_string();
    };
    let Some(last) = raw.lines().last() else {
        return "unknown".to_string();
    };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(last) else {
        return "unknown".to_string();
    };
    parsed
        .get("health_state")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown")
        .to_string()
}

fn enforce_live_policy(
    case: &BrowserAutomationContractCase,
    policy: &BrowserAutomationLivePolicy,
) -> Option<BrowserAutomationReplayResult> {
    let operation = case.operation.trim().to_ascii_lowercase();
    let action_timeout_ms = policy.action_timeout_ms.max(1);
    let max_actions_per_case = policy.max_actions_per_case.max(1);

    if operation == "action" && case.action_repeat_count > max_actions_per_case {
        return Some(BrowserAutomationReplayResult {
            step: BrowserAutomationReplayStep::MalformedInput,
            status_code: 429,
            error_code: Some(BROWSER_AUTOMATION_ERROR_ACTION_LIMIT_EXCEEDED.to_string()),
            response_body: json!({"status":"rejected","reason":"action_limit_exceeded"}),
        });
    }

    if !policy.allow_unsafe_actions && case.unsafe_operation {
        return Some(BrowserAutomationReplayResult {
            step: BrowserAutomationReplayStep::MalformedInput,
            status_code: 403,
            error_code: Some(BROWSER_AUTOMATION_ERROR_UNSAFE_OPERATION.to_string()),
            response_body: json!({"status":"rejected","reason":"unsafe_operation"}),
        });
    }

    if operation == "action" && case.timeout_ms > action_timeout_ms {
        return Some(BrowserAutomationReplayResult {
            step: BrowserAutomationReplayStep::RetryableFailure,
            status_code: 504,
            error_code: Some(BROWSER_AUTOMATION_ERROR_TIMEOUT.to_string()),
            response_body: json!({"status":"retryable","reason":"timeout"}),
        });
    }

    None
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use anyhow::Result;
    use tau_runtime::channel_store::ChannelStore;
    use tempfile::tempdir;

    use super::{
        run_browser_automation_live_fixture, run_browser_automation_live_fixture_with_persistence,
        BrowserActionArtifactBundle, BrowserActionExecutor, BrowserActionRequest,
        BrowserActionResult, BrowserAutomationLivePersistenceConfig, BrowserAutomationLivePolicy,
        BrowserSessionManager, PlaywrightCliActionExecutor,
        BROWSER_AUTOMATION_LIVE_EVENTS_LOG_FILE,
    };
    use crate::browser_automation_contract::{
        parse_browser_automation_contract_fixture, BrowserAutomationCaseExpectation,
        BrowserAutomationContractCase, BrowserAutomationContractFixture,
        BrowserAutomationOutcomeKind, BrowserAutomationReplayStep,
        BROWSER_AUTOMATION_ERROR_BACKEND_UNAVAILABLE, BROWSER_AUTOMATION_ERROR_TIMEOUT,
        BROWSER_AUTOMATION_ERROR_UNSAFE_OPERATION,
    };

    #[derive(Debug, Default)]
    struct ExecutorCounters {
        starts: usize,
        executes: usize,
        shutdowns: usize,
    }

    struct CountingExecutor {
        counters: Arc<Mutex<ExecutorCounters>>,
    }

    impl CountingExecutor {
        fn new(counters: Arc<Mutex<ExecutorCounters>>) -> Self {
            Self { counters }
        }
    }

    impl BrowserActionExecutor for CountingExecutor {
        fn start_session(&mut self) -> Result<()> {
            self.counters.lock().expect("lock").starts += 1;
            Ok(())
        }

        fn execute_action(
            &mut self,
            request: &BrowserActionRequest,
        ) -> Result<BrowserActionResult> {
            self.counters.lock().expect("lock").executes += 1;
            Ok(BrowserActionResult {
                status_code: 200,
                error_code: String::new(),
                response_body: serde_json::json!({
                    "status": "ok",
                    "operation": request.operation,
                    "url": request.url,
                    "title": "Unit fixture page",
                    "dom_nodes": 4,
                }),
                artifacts: BrowserActionArtifactBundle::default(),
            })
        }

        fn shutdown_session(&mut self) -> Result<()> {
            self.counters.lock().expect("lock").shutdowns += 1;
            Ok(())
        }
    }

    fn sample_case(case_id: &str, url: &str) -> BrowserAutomationContractCase {
        BrowserAutomationContractCase {
            schema_version: 1,
            case_id: case_id.to_string(),
            operation: "navigate".to_string(),
            action: String::new(),
            url: url.to_string(),
            selector: String::new(),
            text: String::new(),
            action_repeat_count: 1,
            timeout_ms: 1000,
            unsafe_operation: false,
            simulate_retryable_failure: false,
            simulate_timeout: false,
            expected: BrowserAutomationCaseExpectation {
                outcome: BrowserAutomationOutcomeKind::Success,
                status_code: 200,
                error_code: String::new(),
                response_body: serde_json::json!({
                    "status": "ok",
                    "operation": "navigate",
                    "url": url,
                    "title": "Unit fixture page",
                    "dom_nodes": 4,
                }),
            },
        }
    }

    fn write_mock_playwright_cli(path: &PathBuf) {
        std::fs::write(
            path,
            r#"#!/usr/bin/env python3
import json
import pathlib
import re
import sys

session_file = pathlib.Path(__file__).with_suffix(".session")
command = sys.argv[1] if len(sys.argv) > 1 else ""

if command == "start-session":
    session_file.write_text("active", encoding="utf-8")
    print(json.dumps({"status": "ok"}))
    raise SystemExit(0)

if command == "shutdown-session":
    if session_file.exists():
        session_file.unlink()
    print(json.dumps({"status": "ok"}))
    raise SystemExit(0)

if command != "execute-action":
    print("unsupported command", file=sys.stderr)
    raise SystemExit(2)

payload = json.loads(sys.argv[2]) if len(sys.argv) > 2 else {}
operation = payload.get("operation", "")

if operation == "navigate":
    url = payload.get("url", "")
    if not url.startswith("file://"):
        print(json.dumps({
            "status_code": 400,
            "error_code": "browser_automation_invalid_url",
            "response_body": {"status": "rejected", "reason": "invalid_url"}
        }))
        raise SystemExit(0)
    html_path = pathlib.Path(url[7:])
    html = html_path.read_text(encoding="utf-8")
    match = re.search(r"<title>(.*?)</title>", html, re.IGNORECASE | re.DOTALL)
    title = match.group(1).strip() if match else "Untitled"
    print(json.dumps({
        "status_code": 200,
        "response_body": {
            "status": "ok",
            "operation": "navigate",
            "url": url,
            "title": title,
            "dom_nodes": html.count("<")
        },
        "artifacts": {
            "dom_snapshot_html": html,
            "screenshot_svg": "<svg xmlns='http://www.w3.org/2000/svg'><rect width='10' height='10'/></svg>",
            "trace_json": json.dumps({"events": ["navigate"], "url": url})
        }
    }))
    raise SystemExit(0)

if operation == "snapshot":
    print(json.dumps({
        "status_code": 200,
        "response_body": {
            "status": "ok",
            "operation": "snapshot",
            "snapshot_id": "snapshot-live",
            "elements": [{"id": "e1", "role": "button", "name": "Run"}]
        },
        "artifacts": {
            "dom_snapshot_html": "<html><body><button id='run'>Run</button></body></html>",
            "screenshot_svg": "<svg xmlns='http://www.w3.org/2000/svg'><circle cx='5' cy='5' r='5'/></svg>",
            "trace_json": "{\"events\":[\"snapshot\"]}"
        }
    }))
    raise SystemExit(0)

if operation == "action":
    print(json.dumps({
        "status_code": 200,
        "response_body": {
            "status": "ok",
            "operation": "action",
            "action": payload.get("action", ""),
            "selector": payload.get("selector", ""),
            "repeat_count": payload.get("action_repeat_count", 1),
            "text": payload.get("text", ""),
            "timeout_ms": payload.get("timeout_ms", 0)
        },
        "artifacts": {
            "dom_snapshot_html": "",
            "screenshot_svg": "<svg xmlns='http://www.w3.org/2000/svg'><line x1='0' y1='0' x2='10' y2='10'/></svg>",
            "trace_json": "{\"events\":[\"action\"]}"
        }
    }))
    raise SystemExit(0)

print(json.dumps({
    "status_code": 400,
    "error_code": "browser_automation_invalid_operation",
    "response_body": {"status": "rejected", "reason": "invalid_operation"}
}))
"#,
        )
        .expect("write mock playwright cli");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(path).expect("stat").permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(path, perms).expect("chmod");
        }
    }

    #[test]
    fn unit_session_manager_reuses_started_session_across_multiple_cases() {
        let counters = Arc::new(Mutex::new(ExecutorCounters::default()));
        let mut manager = BrowserSessionManager::new(CountingExecutor::new(counters.clone()));
        let policy = BrowserAutomationLivePolicy::default();
        let case_a = sample_case("case-a", "file:///tmp/a.html");
        let case_b = sample_case("case-b", "file:///tmp/b.html");

        let result_a = manager
            .execute_case(&case_a, &policy)
            .expect("execute case a");
        let result_b = manager
            .execute_case(&case_b, &policy)
            .expect("execute case b");
        manager.shutdown().expect("shutdown");

        assert_eq!(result_a.status_code, 200);
        assert_eq!(result_b.status_code, 200);

        let snapshot = counters.lock().expect("lock");
        assert_eq!(snapshot.starts, 1);
        assert_eq!(snapshot.executes, 2);
        assert_eq!(snapshot.shutdowns, 1);
    }

    #[test]
    fn functional_live_fixture_runner_executes_navigation_snapshot_and_action_sequence() {
        let temp = tempdir().expect("tempdir");
        let page_path = temp.path().join("index.html");
        std::fs::write(
            &page_path,
            "<html><head><title>Live Test Page</title></head><body><button id='run'>Run</button></body></html>",
        )
        .expect("write html");

        let fixture = parse_browser_automation_contract_fixture(&format!(
            r##"{{
  "schema_version": 1,
  "name": "live-sequence",
  "cases": [
    {{
      "schema_version": 1,
      "case_id": "navigate-live",
      "operation": "navigate",
      "url": "file://{}",
      "expected": {{
        "outcome": "success",
        "status_code": 200,
        "response_body": {{
          "status": "ok",
          "operation": "navigate",
          "url": "file://{}",
          "title": "Live Test Page",
          "dom_nodes": 10
        }}
      }}
    }},
    {{
      "schema_version": 1,
      "case_id": "snapshot-live",
      "operation": "snapshot",
      "expected": {{
        "outcome": "success",
        "status_code": 200,
        "response_body": {{
          "status": "ok",
          "operation": "snapshot",
          "snapshot_id": "snapshot-live",
          "elements": [{{"id": "e1", "role": "button", "name": "Run"}}]
        }}
      }}
    }},
    {{
      "schema_version": 1,
      "case_id": "action-live",
      "operation": "action",
      "action": "click",
      "selector": "#run",
      "timeout_ms": 1000,
      "expected": {{
        "outcome": "success",
        "status_code": 200,
        "response_body": {{
          "status": "ok",
          "operation": "action",
          "action": "click",
          "selector": "#run",
          "repeat_count": 1,
          "text": "",
          "timeout_ms": 1000
        }}
      }}
    }}
  ]
}}"##,
            page_path.display(),
            page_path.display()
        ))
        .expect("fixture parse");

        let script_path = temp.path().join("mock-playwright-cli.py");
        write_mock_playwright_cli(&script_path);
        let session_file = script_path.with_extension("session");

        let executor = PlaywrightCliActionExecutor::new(script_path.to_string_lossy().to_string())
            .expect("executor");
        let mut manager = BrowserSessionManager::new(executor);
        let summary = run_browser_automation_live_fixture(
            &fixture,
            &mut manager,
            &BrowserAutomationLivePolicy::default(),
        )
        .expect("live run");

        assert_eq!(summary.discovered_cases, 3);
        assert_eq!(summary.success_cases, 3);
        assert_eq!(summary.malformed_cases, 0);
        assert_eq!(summary.retryable_failures, 0);
        assert_eq!(summary.health_state, "healthy");
        assert_eq!(summary.timeline.len(), 3);
        assert_eq!(summary.timeline[0].case_id, "navigate-live");
        assert_eq!(summary.timeline[1].case_id, "snapshot-live");
        assert_eq!(summary.timeline[2].case_id, "action-live");
        assert_eq!(summary.timeline[0].artifact_types.len(), 3);
        assert_eq!(summary.timeline[1].artifact_types.len(), 3);
        assert_eq!(summary.timeline[2].artifact_types.len(), 2);
        assert!(!session_file.exists());
    }

    #[test]
    fn integration_live_fixture_persists_dom_snapshot_screenshot_and_trace_artifacts() {
        let temp = tempdir().expect("tempdir");
        let page_path = temp.path().join("artifact-page.html");
        std::fs::write(
            &page_path,
            "<html><head><title>Artifact Page</title></head><body><h1>Artifacts</h1></body></html>",
        )
        .expect("write page");
        let fixture = parse_browser_automation_contract_fixture(&format!(
            r##"{{
  "schema_version": 1,
  "name": "artifact-persistence",
  "cases": [
    {{
      "schema_version": 1,
      "case_id": "navigate-artifacts",
      "operation": "navigate",
      "url": "file://{}",
      "expected": {{
        "outcome": "success",
        "status_code": 200,
        "response_body": {{
          "status": "ok",
          "operation": "navigate",
          "url": "file://{}",
          "title": "Artifact Page",
          "dom_nodes": 10
        }}
      }}
    }}
  ]
}}"##,
            page_path.display(),
            page_path.display()
        ))
        .expect("fixture parse");

        let script_path = temp.path().join("mock-playwright-cli.py");
        write_mock_playwright_cli(&script_path);
        let persistence = BrowserAutomationLivePersistenceConfig {
            state_dir: temp.path().join(".tau/browser-live"),
            artifact_retention_days: Some(7),
        };

        let mut manager = BrowserSessionManager::new(
            PlaywrightCliActionExecutor::new(script_path.to_string_lossy().to_string())
                .expect("executor"),
        );
        let summary = run_browser_automation_live_fixture_with_persistence(
            &fixture,
            &mut manager,
            &BrowserAutomationLivePolicy::default(),
            Some(&persistence),
        )
        .expect("live run with persistence");

        assert_eq!(summary.success_cases, 1);
        assert_eq!(summary.artifact_records, 3);
        assert_eq!(summary.timeline.len(), 1);
        assert_eq!(summary.timeline[0].case_id, "navigate-artifacts");
        assert_eq!(summary.timeline[0].artifact_types.len(), 3);
        assert!(summary
            .reason_codes
            .contains(&"artifact_persisted".to_string()));

        let store = ChannelStore::open(
            &persistence.state_dir.join("channel-store"),
            "browser-automation",
            "live",
        )
        .expect("open channel store");
        let loaded = store
            .load_artifact_records_tolerant()
            .expect("load artifacts");
        assert_eq!(loaded.invalid_lines, 0);
        assert_eq!(loaded.records.len(), 3);
        let types = loaded
            .records
            .iter()
            .map(|record| record.artifact_type.clone())
            .collect::<Vec<_>>();
        assert!(types.contains(&"dom_snapshot".to_string()));
        assert!(types.contains(&"screenshot".to_string()));
        assert!(types.contains(&"trace".to_string()));
    }

    #[test]
    fn integration_live_fixture_maps_executor_failures_to_retryable_backend_unavailable() {
        let temp = tempdir().expect("tempdir");
        let script_path = temp.path().join("failing-playwright-cli.sh");
        std::fs::write(
            &script_path,
            "#!/usr/bin/env bash\nset -euo pipefail\nif [[ \"$1\" == \"execute-action\" ]]; then echo 'boom' >&2; exit 9; fi\nexit 0\n",
        )
        .expect("write failing script");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script_path).expect("stat").permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script_path, perms).expect("chmod");
        }

        let fixture = parse_browser_automation_contract_fixture(
            r##"{
  "schema_version": 1,
  "name": "retryable-backend-failure",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "navigate-fails",
      "operation": "navigate",
      "url": "file:///tmp/missing.html",
      "expected": {
        "outcome": "retryable_failure",
        "status_code": 503,
        "error_code": "browser_automation_backend_unavailable",
        "response_body": null
      }
    }
  ]
}"##,
        )
        .expect("fixture parse");

        let executor = PlaywrightCliActionExecutor::new(script_path.to_string_lossy().to_string())
            .expect("executor");
        let mut manager = BrowserSessionManager::new(executor);
        let summary = run_browser_automation_live_fixture(
            &fixture,
            &mut manager,
            &BrowserAutomationLivePolicy::default(),
        )
        .expect("run should map command failure");
        assert_eq!(summary.retryable_failures, 1);
        assert_eq!(summary.health_state, "degraded");
        assert!(summary
            .reason_codes
            .contains(&"live_executor_failures_observed".to_string()));
    }

    #[test]
    fn regression_drop_cleanup_shuts_down_active_session_without_orphan_marker() {
        let temp = tempdir().expect("tempdir");
        let page_path = temp.path().join("page.html");
        std::fs::write(
            &page_path,
            "<html><head><title>Cleanup Page</title></head><body></body></html>",
        )
        .expect("write page");

        let script_path = temp.path().join("mock-playwright-cli.py");
        write_mock_playwright_cli(&script_path);
        let session_file = script_path.with_extension("session");

        let case = sample_case("cleanup-case", &format!("file://{}", page_path.display()));
        let mut manager = BrowserSessionManager::new(
            PlaywrightCliActionExecutor::new(script_path.to_string_lossy().to_string())
                .expect("executor"),
        );
        let result = manager
            .execute_case(&case, &BrowserAutomationLivePolicy::default())
            .expect("execute case");
        assert_eq!(result.step, BrowserAutomationReplayStep::Success);
        assert!(session_file.exists());

        drop(manager);
        assert!(!session_file.exists());
    }

    #[test]
    fn regression_policy_guardrails_prevent_executor_calls_for_timeout_and_unsafe_cases() {
        let counters = Arc::new(Mutex::new(ExecutorCounters::default()));
        let mut manager = BrowserSessionManager::new(CountingExecutor::new(counters.clone()));
        let policy = BrowserAutomationLivePolicy {
            action_timeout_ms: 10,
            max_actions_per_case: 1,
            allow_unsafe_actions: false,
        };
        let mut timeout_case = sample_case("timeout-case", "file:///tmp/timeout.html");
        timeout_case.operation = "action".to_string();
        timeout_case.action = "wait".to_string();
        timeout_case.selector = "#ready".to_string();
        timeout_case.timeout_ms = 99;

        let mut unsafe_case = sample_case("unsafe-case", "file:///tmp/unsafe.html");
        unsafe_case.operation = "action".to_string();
        unsafe_case.action = "click".to_string();
        unsafe_case.selector = "#delete".to_string();
        unsafe_case.unsafe_operation = true;

        let timeout = manager
            .execute_case(&timeout_case, &policy)
            .expect("timeout case");
        let unsafe_result = manager
            .execute_case(&unsafe_case, &policy)
            .expect("unsafe case");

        assert_eq!(
            timeout.error_code.as_deref(),
            Some(BROWSER_AUTOMATION_ERROR_TIMEOUT)
        );
        assert_eq!(
            unsafe_result.error_code.as_deref(),
            Some(BROWSER_AUTOMATION_ERROR_UNSAFE_OPERATION)
        );

        let snapshot = counters.lock().expect("lock");
        assert_eq!(snapshot.starts, 0);
        assert_eq!(snapshot.executes, 0);
        assert_eq!(snapshot.shutdowns, 0);
    }

    #[test]
    fn regression_policy_denials_surface_reason_codes_with_deterministic_counts() {
        let fixture = parse_browser_automation_contract_fixture(
            r##"{
  "schema_version": 1,
  "name": "policy-denials",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "action-cap",
      "operation": "action",
      "action": "click",
      "selector": "#go",
      "action_repeat_count": 2,
      "expected": {
        "outcome": "malformed_input",
        "status_code": 429,
        "error_code": "browser_automation_action_limit_exceeded",
        "response_body": {"status":"rejected","reason":"action_limit_exceeded"}
      }
    },
    {
      "schema_version": 1,
      "case_id": "unsafe",
      "operation": "action",
      "action": "click",
      "selector": "#delete",
      "unsafe_operation": true,
      "expected": {
        "outcome": "malformed_input",
        "status_code": 403,
        "error_code": "browser_automation_unsafe_operation",
        "response_body": {"status":"rejected","reason":"unsafe_operation"}
      }
    }
  ]
}"##,
        )
        .expect("fixture parse");

        let counters = Arc::new(Mutex::new(ExecutorCounters::default()));
        let mut manager = BrowserSessionManager::new(CountingExecutor::new(counters.clone()));
        let policy = BrowserAutomationLivePolicy {
            action_timeout_ms: 1000,
            max_actions_per_case: 1,
            allow_unsafe_actions: false,
        };
        let summary =
            run_browser_automation_live_fixture(&fixture, &mut manager, &policy).expect("run");

        assert_eq!(summary.malformed_cases, 2);
        assert_eq!(summary.denied_action_limit, 1);
        assert_eq!(summary.denied_unsafe_actions, 1);
        assert!(summary
            .reason_codes
            .contains(&"action_limit_guardrail_denied".to_string()));
        assert!(summary
            .reason_codes
            .contains(&"unsafe_actions_denied".to_string()));

        let snapshot = counters.lock().expect("lock");
        assert_eq!(snapshot.starts, 0);
        assert_eq!(snapshot.executes, 0);
    }

    #[test]
    fn regression_live_timeline_preserves_fixture_case_order_for_compatibility() {
        let fixture = BrowserAutomationContractFixture {
            schema_version: 1,
            name: "timeline-order".to_string(),
            description: "ensures summary timeline order matches fixture order".to_string(),
            cases: vec![
                sample_case("z-case", "file:///tmp/z.html"),
                sample_case("a-case", "file:///tmp/a.html"),
            ],
        };

        let counters = Arc::new(Mutex::new(ExecutorCounters::default()));
        let mut manager = BrowserSessionManager::new(CountingExecutor::new(counters.clone()));
        let summary = run_browser_automation_live_fixture(
            &fixture,
            &mut manager,
            &BrowserAutomationLivePolicy::default(),
        )
        .expect("run");
        assert_eq!(summary.timeline.len(), 2);
        assert_eq!(summary.timeline[0].case_id, "z-case");
        assert_eq!(summary.timeline[1].case_id, "a-case");

        let snapshot = counters.lock().expect("lock");
        assert_eq!(snapshot.starts, 1);
        assert_eq!(snapshot.executes, 2);
    }

    #[test]
    fn integration_live_cycle_report_tracks_health_state_transitions() {
        let temp = tempdir().expect("tempdir");
        let persistence = BrowserAutomationLivePersistenceConfig {
            state_dir: temp.path().join(".tau/browser-live-transition"),
            artifact_retention_days: Some(1),
        };
        let failing_script = temp.path().join("failing-playwright-cli.sh");
        std::fs::write(
            &failing_script,
            "#!/usr/bin/env bash\nset -euo pipefail\nif [[ \"$1\" == \"execute-action\" ]]; then exit 9; fi\nexit 0\n",
        )
        .expect("write failing script");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&failing_script)
                .expect("stat")
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&failing_script, perms).expect("chmod");
        }

        let failure_fixture = parse_browser_automation_contract_fixture(
            r##"{
  "schema_version": 1,
  "name": "fail-first",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "backend-fail",
      "operation": "navigate",
      "url": "file:///tmp/missing.html",
      "expected": {
        "outcome": "retryable_failure",
        "status_code": 503,
        "error_code": "browser_automation_backend_unavailable",
        "response_body": null
      }
    }
  ]
}"##,
        )
        .expect("failure fixture parse");

        let mut fail_manager = BrowserSessionManager::new(
            PlaywrightCliActionExecutor::new(failing_script.to_string_lossy().to_string())
                .expect("executor"),
        );
        let failed = run_browser_automation_live_fixture_with_persistence(
            &failure_fixture,
            &mut fail_manager,
            &BrowserAutomationLivePolicy::default(),
            Some(&persistence),
        )
        .expect("failed live run");
        assert_eq!(failed.health_state, "degraded");

        let success_page = temp.path().join("success.html");
        std::fs::write(
            &success_page,
            "<html><head><title>Recovery Page</title></head><body></body></html>",
        )
        .expect("write success page");
        let success_fixture = parse_browser_automation_contract_fixture(&format!(
            r##"{{
  "schema_version": 1,
  "name": "recovery",
  "cases": [
    {{
      "schema_version": 1,
      "case_id": "navigate-recovery",
      "operation": "navigate",
      "url": "file://{}",
      "expected": {{
        "outcome": "success",
        "status_code": 200,
        "response_body": {{
          "status": "ok",
          "operation": "navigate",
          "url": "file://{}",
          "title": "Recovery Page",
          "dom_nodes": 8
        }}
      }}
    }}
  ]
}}"##,
            success_page.display(),
            success_page.display()
        ))
        .expect("success fixture parse");

        let success_script = temp.path().join("mock-playwright-cli.py");
        write_mock_playwright_cli(&success_script);
        let mut success_manager = BrowserSessionManager::new(
            PlaywrightCliActionExecutor::new(success_script.to_string_lossy().to_string())
                .expect("executor"),
        );
        let recovered = run_browser_automation_live_fixture_with_persistence(
            &success_fixture,
            &mut success_manager,
            &BrowserAutomationLivePolicy::default(),
            Some(&persistence),
        )
        .expect("recovery run");
        assert_eq!(recovered.health_state, "healthy");

        let events_log = std::fs::read_to_string(
            persistence
                .state_dir
                .join(BROWSER_AUTOMATION_LIVE_EVENTS_LOG_FILE),
        )
        .expect("events log");
        let last = events_log.lines().last().expect("last event");
        assert!(last.contains("\"health_transition\":\"degraded->healthy\""));
    }

    #[test]
    fn unit_playwright_executor_rejects_empty_cli_path() {
        let error = PlaywrightCliActionExecutor::new("  ").expect_err("empty path should fail");
        assert!(error.to_string().contains("cannot be empty"));
    }

    #[test]
    fn integration_backend_failure_result_uses_expected_error_code() {
        let result = BrowserActionResult {
            status_code: 503,
            error_code: BROWSER_AUTOMATION_ERROR_BACKEND_UNAVAILABLE.to_string(),
            response_body: serde_json::json!({"status":"retryable","reason":"backend_unavailable"}),
            artifacts: BrowserActionArtifactBundle::default(),
        };
        assert_eq!(
            result.to_replay_step(),
            BrowserAutomationReplayStep::RetryableFailure
        );
        assert_eq!(
            result.error_code,
            BROWSER_AUTOMATION_ERROR_BACKEND_UNAVAILABLE
        );
    }
}
