//! REPL process harness integration coverage with deterministic scripted fixtures.

use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, ExitStatus, Stdio},
    thread,
    time::Duration,
};

use httpmock::prelude::*;
use serde::Deserialize;
use serde_json::json;
use tempfile::tempdir;
use wait_timeout::ChildExt;

use super::*;

const REPL_HARNESS_SCHEMA_VERSION: u32 = 1;
const REPL_PROMPT: &str = "tau> ";
const DEFAULT_TIMEOUT_MS: u64 = 8_000;
const MIN_TIMEOUT_MS: u64 = 1_500;
const TIMEOUT_OVERRIDE_ENV: &str = "TAU_REPL_HARNESS_TIMEOUT_MS";

#[derive(Debug, Deserialize)]
struct ReplHarnessFixture {
    schema_version: u32,
    name: String,
    args: Vec<String>,
    stdin_script: String,
    timeout_ms: u64,
    expect: ReplHarnessExpectation,
}

#[derive(Debug, Deserialize)]
struct ReplHarnessExpectation {
    success: bool,
    #[serde(default)]
    min_prompt_count: Option<usize>,
    #[serde(default)]
    stdout_contains: Vec<String>,
    #[serde(default)]
    stdout_not_contains: Vec<String>,
    #[serde(default)]
    stderr_contains: Vec<String>,
    #[serde(default)]
    stderr_not_contains: Vec<String>,
}

#[derive(Debug)]
struct ReplHarnessResult {
    args: Vec<String>,
    stdin_script: String,
    timeout: Duration,
    timed_out: bool,
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

fn repl_harness_fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join("repl-harness")
        .join(name)
}

fn load_repl_harness_fixture(name: &str) -> ReplHarnessFixture {
    let path = repl_harness_fixture_path(name);
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    let fixture = serde_json::from_str::<ReplHarnessFixture>(&raw)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()));
    assert_eq!(
        fixture.schema_version,
        REPL_HARNESS_SCHEMA_VERSION,
        "unsupported fixture schema_version in {}",
        path.display()
    );
    fixture
}

fn render_template(raw: &str, vars: &BTreeMap<&str, String>) -> String {
    vars.iter().fold(raw.to_string(), |acc, (key, value)| {
        acc.replace(&format!("{{{{{key}}}}}"), value)
    })
}

fn effective_timeout(fixture_timeout_ms: u64) -> Duration {
    let override_ms = std::env::var(TIMEOUT_OVERRIDE_ENV)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_TIMEOUT_MS);
    Duration::from_millis(fixture_timeout_ms.max(MIN_TIMEOUT_MS).max(override_ms))
}

fn run_repl_fixture(
    name: &str,
    vars: &BTreeMap<&str, String>,
) -> (ReplHarnessFixture, ReplHarnessResult) {
    let fixture = load_repl_harness_fixture(name);
    let args = fixture
        .args
        .iter()
        .map(|arg| render_template(arg, vars))
        .collect::<Vec<_>>();
    let stdin_script = render_template(&fixture.stdin_script, vars);
    let timeout = effective_timeout(fixture.timeout_ms);

    let temp = tempdir().expect("tempdir");
    let mut command = ProcessCommand::new(assert_cmd::cargo::cargo_bin!("tau-coding-agent"));
    command
        .current_dir(temp.path())
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().expect("spawn tau-coding-agent");

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(stdin_script.as_bytes())
            .expect("write stdin script");
    }

    let mut stdout_pipe = child.stdout.take().expect("child stdout piped");
    let stdout_join = thread::spawn(move || {
        let mut output = Vec::new();
        stdout_pipe
            .read_to_end(&mut output)
            .expect("read child stdout");
        output
    });

    let mut stderr_pipe = child.stderr.take().expect("child stderr piped");
    let stderr_join = thread::spawn(move || {
        let mut output = Vec::new();
        stderr_pipe
            .read_to_end(&mut output)
            .expect("read child stderr");
        output
    });

    let (status, timed_out) = match child.wait_timeout(timeout).expect("wait with timeout") {
        Some(status) => (status, false),
        None => {
            child.kill().expect("kill timed-out process");
            (child.wait().expect("wait after kill"), true)
        }
    };

    let stdout_bytes = stdout_join.join().expect("join stdout reader");
    let stderr_bytes = stderr_join.join().expect("join stderr reader");
    let result = ReplHarnessResult {
        args,
        stdin_script,
        timeout,
        timed_out,
        status,
        stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
        stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
    };

    (fixture, result)
}

fn assert_repl_fixture(fixture: &ReplHarnessFixture, result: &ReplHarnessResult) {
    let diagnostics = format!(
        "fixture={}\nargs={:?}\ntimeout_ms={}\ntimed_out={}\nexit_status={:?}\nstdin_script=\n{}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        fixture.name,
        result.args,
        result.timeout.as_millis(),
        result.timed_out,
        result.status,
        result.stdin_script,
        result.stdout,
        result.stderr,
    );

    assert!(
        !result.timed_out,
        "fixture process exceeded timeout and was killed\n{diagnostics}"
    );
    if fixture.expect.success {
        assert!(
            result.status.success(),
            "fixture expected success but exited non-zero\n{diagnostics}"
        );
    } else {
        assert!(
            !result.status.success(),
            "fixture expected failure but exited successfully\n{diagnostics}"
        );
    }

    if let Some(min_prompt_count) = fixture.expect.min_prompt_count {
        let prompt_count = result.stdout.matches(REPL_PROMPT).count();
        assert!(
            prompt_count >= min_prompt_count,
            "fixture expected at least {min_prompt_count} prompts, observed {prompt_count}\n{diagnostics}"
        );
    }

    for needle in &fixture.expect.stdout_contains {
        assert!(
            result.stdout.contains(needle),
            "missing stdout fragment: {needle:?}\n{diagnostics}"
        );
    }
    for needle in &fixture.expect.stdout_not_contains {
        assert!(
            !result.stdout.contains(needle),
            "unexpected stdout fragment present: {needle:?}\n{diagnostics}"
        );
    }
    for needle in &fixture.expect.stderr_contains {
        assert!(
            result.stderr.contains(needle),
            "missing stderr fragment: {needle:?}\n{diagnostics}"
        );
    }
    for needle in &fixture.expect.stderr_not_contains {
        assert!(
            !result.stderr.contains(needle),
            "unexpected stderr fragment present: {needle:?}\n{diagnostics}"
        );
    }
}

#[test]
fn unit_repl_harness_fixture_schema_guard_accepts_v1() {
    let fixture = load_repl_harness_fixture("help-and-unknown-command.json");
    assert_eq!(fixture.schema_version, REPL_HARNESS_SCHEMA_VERSION);
    assert_eq!(fixture.name, "help-and-unknown-command");
}

#[test]
fn functional_repl_harness_executes_help_and_unknown_command_script() {
    let vars = BTreeMap::new();
    let (fixture, result) = run_repl_fixture("help-and-unknown-command.json", &vars);
    assert_repl_fixture(&fixture, &result);
}

#[test]
fn integration_repl_harness_executes_prompt_flow_with_mock_openai() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "repl harness integration response"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 6, "completion_tokens": 4, "total_tokens": 10}
        }));
    });

    let mut vars = BTreeMap::new();
    vars.insert("API_BASE", server.base_url());
    let (fixture, result) = run_repl_fixture("prompt-happy-path.json", &vars);
    assert_repl_fixture(&fixture, &result);
    openai.assert_calls(1);
}

#[test]
fn regression_repl_harness_handles_eof_exit_without_hanging() {
    let vars = BTreeMap::new();
    let (fixture, result) = run_repl_fixture("eof-exit.json", &vars);
    assert_repl_fixture(&fixture, &result);
}

#[test]
fn regression_repl_harness_turn_timeout_remains_deterministic_in_ci() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200)
            .delay(Duration::from_millis(150))
            .json_body(json!({
                "choices": [{
                    "message": {"content": "delayed response"},
                    "finish_reason": "stop"
                }],
                "usage": {"prompt_tokens": 4, "completion_tokens": 2, "total_tokens": 6}
            }));
    });

    let mut vars = BTreeMap::new();
    vars.insert("API_BASE", server.base_url());
    let (fixture, result) = run_repl_fixture("prompt-turn-timeout.json", &vars);
    assert_repl_fixture(&fixture, &result);
    openai.assert_calls(1);
}
