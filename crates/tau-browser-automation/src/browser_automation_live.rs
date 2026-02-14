use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::browser_automation_contract::{
    assert_browser_automation_result_matches_expectation, BrowserAutomationContractCase,
    BrowserAutomationContractFixture, BrowserAutomationReplayResult, BrowserAutomationReplayStep,
    BROWSER_AUTOMATION_ERROR_ACTION_LIMIT_EXCEEDED, BROWSER_AUTOMATION_ERROR_BACKEND_UNAVAILABLE,
    BROWSER_AUTOMATION_ERROR_TIMEOUT, BROWSER_AUTOMATION_ERROR_UNSAFE_OPERATION,
};

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `BrowserActionResult` used across Tau components.
pub struct BrowserActionResult {
    pub status_code: u16,
    #[serde(default)]
    pub error_code: String,
    #[serde(default)]
    pub response_body: serde_json::Value,
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

    fn into_replay_result(self) -> BrowserAutomationReplayResult {
        BrowserAutomationReplayResult {
            step: self.to_replay_step(),
            status_code: self.status_code,
            error_code: if self.error_code.trim().is_empty() {
                None
            } else {
                Some(self.error_code)
            },
            response_body: self.response_body,
        }
    }
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
        if let Some(policy_result) = enforce_live_policy(case, policy) {
            return Ok(policy_result);
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
            },
        };
        Ok(action_result.into_replay_result())
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
/// Public struct `BrowserAutomationLiveRunSummary` used across Tau components.
pub struct BrowserAutomationLiveRunSummary {
    pub discovered_cases: usize,
    pub success_cases: usize,
    pub malformed_cases: usize,
    pub retryable_failures: usize,
}

pub fn run_browser_automation_live_fixture<E: BrowserActionExecutor>(
    fixture: &BrowserAutomationContractFixture,
    manager: &mut BrowserSessionManager<E>,
    policy: &BrowserAutomationLivePolicy,
) -> Result<BrowserAutomationLiveRunSummary> {
    let mut summary = BrowserAutomationLiveRunSummary {
        discovered_cases: fixture.cases.len(),
        ..BrowserAutomationLiveRunSummary::default()
    };

    for case in &fixture.cases {
        let result = manager.execute_case(case, policy)?;
        assert_browser_automation_result_matches_expectation(case, &result)?;
        match result.step {
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
    }

    manager.shutdown()?;
    Ok(summary)
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
    use tempfile::tempdir;

    use super::{
        run_browser_automation_live_fixture, BrowserActionExecutor, BrowserActionRequest,
        BrowserActionResult, BrowserAutomationLivePolicy, BrowserSessionManager,
        PlaywrightCliActionExecutor,
    };
    use crate::browser_automation_contract::{
        parse_browser_automation_contract_fixture, BrowserAutomationCaseExpectation,
        BrowserAutomationContractCase, BrowserAutomationOutcomeKind, BrowserAutomationReplayStep,
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
    fn functional_live_fixture_runner_executes_navigation_and_action_sequence() {
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

        assert_eq!(summary.discovered_cases, 2);
        assert_eq!(summary.success_cases, 2);
        assert_eq!(summary.malformed_cases, 0);
        assert_eq!(summary.retryable_failures, 0);
        assert!(!session_file.exists());
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
        }
        .into_replay_result();
        assert_eq!(result.step, BrowserAutomationReplayStep::RetryableFailure);
        assert_eq!(
            result.error_code.as_deref(),
            Some(BROWSER_AUTOMATION_ERROR_BACKEND_UNAVAILABLE)
        );
    }
}
