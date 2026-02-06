use std::fs;

use assert_cmd::Command;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{Signer, SigningKey};
use httpmock::prelude::*;
use predicates::prelude::*;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use tempfile::tempdir;

#[derive(Debug, Deserialize)]
struct SessionEntry {
    id: u64,
    parent_id: Option<u64>,
    message: SessionMessage,
}

#[derive(Debug, Deserialize)]
struct SessionMessage {
    role: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "record_type", rename_all = "snake_case")]
enum SessionRecord {
    Meta {
        schema_version: u32,
    },
    Entry {
        id: u64,
        parent_id: Option<u64>,
        message: SessionMessage,
    },
}

fn parse_session_entries(path: &std::path::Path) -> Vec<SessionEntry> {
    let raw = fs::read_to_string(path).expect("session file should exist");
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| {
            let record = serde_json::from_str::<SessionRecord>(line).expect("line should parse");
            match record {
                SessionRecord::Meta { schema_version } => {
                    assert_eq!(schema_version, 1);
                    None
                }
                SessionRecord::Entry {
                    id,
                    parent_id,
                    message,
                } => Some(SessionEntry {
                    id,
                    parent_id,
                    message,
                }),
            }
        })
        .collect()
}

fn binary_command() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("pi-coding-agent"))
}

#[test]
fn help_hides_environment_variable_values() {
    let mut cmd = binary_command();
    cmd.arg("--help")
        .env("OPENAI_API_KEY", "SUPER_SECRET_TEST_TOKEN_123")
        .env("ANTHROPIC_API_KEY", "SUPER_SECRET_ANTHROPIC_456");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("OPENAI_API_KEY"))
        .stdout(predicate::str::contains("ANTHROPIC_API_KEY"))
        .stdout(predicate::str::contains("SUPER_SECRET_TEST_TOKEN_123").not())
        .stdout(predicate::str::contains("SUPER_SECRET_ANTHROPIC_456").not());
}

#[test]
fn no_session_and_branch_from_combination_fails_fast() {
    let mut cmd = binary_command();
    cmd.args(["--no-session", "--branch-from", "1", "--prompt", "hello"]);

    cmd.assert().failure().stderr(predicate::str::contains(
        "--branch-from cannot be used together with --no-session",
    ));
}

#[test]
fn session_validate_flag_succeeds_for_valid_session_file() {
    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("session.jsonl");
    let raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"sys"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":2,
            "parent_id":1,
            "message":{
                "role":"user",
                "content":[{"type":"text","text":"hello"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&session, format!("{raw}\n")).expect("write valid session");

    let mut cmd = binary_command();
    cmd.args([
        "--session",
        session.to_str().expect("utf8 path"),
        "--session-validate",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("session validation passed"))
        .stdout(predicate::str::contains("entries=2"));
}

#[test]
fn regression_session_validate_flag_fails_for_invalid_session_file() {
    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("session.jsonl");
    let raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":2,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"sys"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":2,
            "parent_id":1,
            "message":{
                "role":"user",
                "content":[{"type":"text","text":"cycle"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&session, format!("{raw}\n")).expect("write invalid session");

    let mut cmd = binary_command();
    cmd.args([
        "--session",
        session.to_str().expect("utf8 path"),
        "--session-validate",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("session validation failed"))
        .stderr(predicate::str::contains("cycles=2"));
}

#[test]
fn interactive_help_and_unknown_command_suggestions_work() {
    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--no-session",
    ])
    .write_stdin("/help\n/help branch\n/help polciy\n/polciy\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("commands:"))
        .stdout(predicate::str::contains("usage: /branch <id>"))
        .stdout(predicate::str::contains("unknown help topic: /polciy"))
        .stdout(predicate::str::contains("did you mean /policy?"));
}

#[test]
fn interactive_session_import_merge_remaps_collisions_by_default() {
    let temp = tempdir().expect("tempdir");
    let target = temp.path().join("target.jsonl");
    let source = temp.path().join("source.jsonl");

    let target_raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"target-root"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&target, format!("{target_raw}\n")).expect("write target");

    let source_raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"import-root"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":2,
            "parent_id":1,
            "message":{
                "role":"user",
                "content":[{"type":"text","text":"import-user"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&source, format!("{source_raw}\n")).expect("write source");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--session",
        target.to_str().expect("utf8 target path"),
    ])
    .write_stdin(format!("/session-import {}\n/quit\n", source.display()));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("session import complete"))
        .stdout(predicate::str::contains("mode=merge"))
        .stdout(predicate::str::contains("remapped_entries=2"))
        .stdout(predicate::str::contains("remapped_ids=1->2,2->3"));

    let entries = parse_session_entries(&target);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].id, 1);
    assert_eq!(entries[1].id, 2);
    assert_eq!(entries[1].parent_id, None);
    assert_eq!(entries[2].id, 3);
    assert_eq!(entries[2].parent_id, Some(2));
}

#[test]
fn integration_interactive_session_import_replace_mode_overwrites_target() {
    let temp = tempdir().expect("tempdir");
    let target = temp.path().join("target-replace.jsonl");
    let source = temp.path().join("source-replace.jsonl");

    let target_raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"target-root"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":2,
            "parent_id":1,
            "message":{
                "role":"user",
                "content":[{"type":"text","text":"target-user"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&target, format!("{target_raw}\n")).expect("write target");

    let source_raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":10,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"import-root"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":11,
            "parent_id":10,
            "message":{
                "role":"assistant",
                "content":[{"type":"text","text":"import-assistant"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&source, format!("{source_raw}\n")).expect("write source");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--session",
        target.to_str().expect("utf8 target path"),
        "--session-import-mode",
        "replace",
    ])
    .write_stdin(format!(
        "/session-import {}\n/session\n/quit\n",
        source.display()
    ));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("session import complete"))
        .stdout(predicate::str::contains("mode=replace"))
        .stdout(predicate::str::contains("remapped_ids=none"))
        .stdout(predicate::str::contains("replaced_entries=2"));

    let entries = parse_session_entries(&target);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].id, 10);
    assert_eq!(entries[0].parent_id, None);
    assert_eq!(entries[1].id, 11);
    assert_eq!(entries[1].parent_id, Some(10));
}

#[test]
fn regression_session_repair_reports_removed_ids() {
    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("repair-session.jsonl");

    let raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"root"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":2,
            "parent_id":99,
            "message":{
                "role":"user",
                "content":[{"type":"text","text":"invalid-parent"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":3,
            "parent_id":4,
            "message":{
                "role":"user",
                "content":[{"type":"text","text":"cycle-a"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":4,
            "parent_id":3,
            "message":{
                "role":"assistant",
                "content":[{"type":"text","text":"cycle-b"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":5,
            "parent_id":1,
            "message":{
                "role":"user",
                "content":[{"type":"text","text":"healthy-head"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&session, format!("{raw}\n")).expect("write malformed session");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--session",
        session.to_str().expect("utf8 session path"),
    ])
    .write_stdin("/session-repair\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("repair complete"))
        .stdout(predicate::str::contains("invalid_parent_ids=2"))
        .stdout(predicate::str::contains("cycle_ids=3,4"));

    let entries = parse_session_entries(&session);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].id, 1);
    assert_eq!(entries[1].id, 5);
}

#[test]
fn openai_prompt_persists_session_and_supports_branch_from() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "integration openai response"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 3, "total_tokens": 13}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("session.jsonl");

    let mut first = binary_command();
    first.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "first prompt",
        "--session",
        session.to_str().expect("utf8 session path"),
    ]);

    first
        .assert()
        .success()
        .stdout(predicate::str::contains("integration openai response"));

    let entries = parse_session_entries(&session);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].message.role, "system");
    assert_eq!(entries[1].message.role, "user");
    assert_eq!(entries[2].message.role, "assistant");

    let mut second = binary_command();
    second.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "forked prompt",
        "--session",
        session.to_str().expect("utf8 session path"),
        "--branch-from",
        "2",
    ]);

    second.assert().success();

    let entries = parse_session_entries(&session);
    assert_eq!(entries.len(), 5);
    assert_eq!(entries[3].parent_id, Some(2));
    assert_eq!(entries[4].parent_id, Some(entries[3].id));

    openai.assert_calls(2);
}

#[test]
fn anthropic_prompt_works_end_to_end() {
    let server = MockServer::start();
    let anthropic = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/messages")
            .header("x-api-key", "test-anthropic-key")
            .header("anthropic-version", "2023-06-01");
        then.status(200).json_body(json!({
            "content": [{"type": "text", "text": "integration anthropic response"}],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 8, "output_tokens": 3}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "anthropic/claude-sonnet-4-20250514",
        "--anthropic-api-base",
        &format!("{}/v1", server.base_url()),
        "--anthropic-api-key",
        "test-anthropic-key",
        "--prompt",
        "hello",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("integration anthropic response"));

    anthropic.assert_calls(1);
}

#[test]
fn google_prompt_works_end_to_end() {
    let server = MockServer::start();
    let google = server.mock(|when, then| {
        when.method(POST)
            .path("/models/gemini-2.5-pro:generateContent")
            .query_param("key", "test-google-key");
        then.status(200).json_body(json!({
            "candidates": [{
                "content": {
                    "parts": [{"text": "integration google response"}]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 8,
                "candidatesTokenCount": 3,
                "totalTokenCount": 11
            }
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "google/gemini-2.5-pro",
        "--google-api-base",
        &server.base_url(),
        "--google-api-key",
        "test-google-key",
        "--prompt",
        "hello",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("integration google response"));

    google.assert_calls(1);
}

#[test]
fn stream_output_flags_are_accepted_in_prompt_mode() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "streamed response"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 4, "completion_tokens": 2, "total_tokens": 6}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--no-session",
        "--stream-output",
        "false",
        "--stream-delay-ms",
        "0",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("streamed response"));

    openai.assert_calls(1);
}

#[test]
fn bash_profile_flags_are_accepted_in_prompt_mode() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "profile ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 4, "completion_tokens": 2, "total_tokens": 6}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--bash-profile",
        "strict",
        "--allow-command",
        "python,cargo-nextest*",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("profile ok"));

    openai.assert_calls(1);
}

#[test]
fn session_lock_flags_are_accepted_in_prompt_mode() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "lock flags ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 4, "completion_tokens": 2, "total_tokens": 6}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--session-lock-wait-ms",
        "250",
        "--session-lock-stale-ms",
        "0",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("lock flags ok"));

    openai.assert_calls(1);
}

#[test]
fn print_tool_policy_flag_outputs_effective_policy_json() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "policy output ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 4, "completion_tokens": 2, "total_tokens": 6}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--print-tool-policy",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"max_file_write_bytes\""))
        .stdout(predicate::str::contains("\"os_sandbox_mode\""))
        .stdout(predicate::str::contains("policy output ok"));

    openai.assert_calls(1);
}

#[test]
fn prompt_file_flag_runs_one_shot_prompt() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .body_includes("prompt from file");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "file prompt ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 4, "completion_tokens": 2, "total_tokens": 6}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let prompt_path = temp.path().join("prompt.txt");
    fs::write(&prompt_path, "prompt from file").expect("write prompt");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt-file",
        prompt_path.to_str().expect("utf8 path"),
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("file prompt ok"));

    openai.assert_calls(1);
}

#[test]
fn prompt_file_dash_reads_prompt_from_stdin() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .body_includes("prompt from stdin");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "stdin prompt ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 4, "completion_tokens": 2, "total_tokens": 6}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt-file",
        "-",
        "--no-session",
    ])
    .write_stdin("prompt from stdin");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("stdin prompt ok"));

    openai.assert_calls(1);
}

#[test]
fn regression_prompt_file_dash_rejects_empty_stdin() {
    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--prompt-file",
        "-",
        "--no-session",
    ])
    .write_stdin(" \n\t");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("stdin prompt"))
        .stderr(predicate::str::contains("is empty"));
}

#[test]
fn regression_empty_prompt_file_fails_fast() {
    let temp = tempdir().expect("tempdir");
    let prompt_path = temp.path().join("empty-prompt.txt");
    fs::write(&prompt_path, " \n\t").expect("write prompt");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--prompt-file",
        prompt_path.to_str().expect("utf8 path"),
        "--no-session",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("prompt file"))
        .stderr(predicate::str::contains("is empty"));
}

#[test]
fn system_prompt_file_flag_overrides_inline_system_prompt() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .body_includes("system prompt from file")
            .body_excludes(
                "You are a focused coding assistant. Prefer concrete steps and safe edits.",
            );
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "system prompt file ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 4, "completion_tokens": 2, "total_tokens": 6}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let system_prompt_path = temp.path().join("system-prompt.txt");
    fs::write(&system_prompt_path, "system prompt from file").expect("write system prompt");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--system-prompt-file",
        system_prompt_path.to_str().expect("utf8 path"),
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("system prompt file ok"));

    openai.assert_calls(1);
}

#[test]
fn regression_empty_system_prompt_file_fails_fast() {
    let temp = tempdir().expect("tempdir");
    let system_prompt_path = temp.path().join("empty-system-prompt.txt");
    fs::write(&system_prompt_path, "  \n\t").expect("write system prompt");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--system-prompt-file",
        system_prompt_path.to_str().expect("utf8 path"),
        "--no-session",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("system prompt file"))
        .stderr(predicate::str::contains("is empty"));
}

#[test]
fn tool_audit_log_flag_creates_audit_log_file() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "audit log ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 4, "completion_tokens": 2, "total_tokens": 6}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let audit_path = temp.path().join("tool-audit.jsonl");
    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--tool-audit-log",
        audit_path.to_str().expect("utf8 path"),
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("audit log ok"));

    assert!(audit_path.exists());
    openai.assert_calls(1);
}

#[test]
fn turn_timeout_flag_times_out_prompt_and_keeps_process_healthy() {
    let server = MockServer::start();
    let _openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200)
            .delay(std::time::Duration::from_millis(150))
            .json_body(json!({
                "choices": [{
                    "message": {"content": "slow response"},
                    "finish_reason": "stop"
                }],
                "usage": {"prompt_tokens": 4, "completion_tokens": 2, "total_tokens": 6}
            }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--turn-timeout-ms",
        "20",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("request timed out"));
}

#[test]
fn selected_skill_is_included_in_system_prompt() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .json_body_includes(
                json!({
                    "messages": [{
                        "role": "system",
                        "content": "base\n\n# Skill: focus\nAlways use checklist"
                    }]
                })
                .to_string(),
            );
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 6, "completion_tokens": 1, "total_tokens": 7}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("create skills dir");
    fs::write(skills_dir.join("focus.md"), "Always use checklist").expect("write skill file");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--system-prompt",
        "base",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skill",
        "focus",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("ok"));
    openai.assert_calls(1);
}

#[test]
fn install_skill_flag_installs_skill_before_prompt() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .json_body_includes(
                json!({
                    "messages": [{
                        "role": "system",
                        "content": "base\n\n# Skill: installable\nInstalled skill body"
                    }]
                })
                .to_string(),
            );
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "ok install"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 6, "completion_tokens": 1, "total_tokens": 7}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    let source_skill = temp.path().join("installable.md");
    fs::write(&source_skill, "Installed skill body").expect("write source skill");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--system-prompt",
        "base",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--install-skill",
        source_skill.to_str().expect("utf8 path"),
        "--skill",
        "installable",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills install: installed=1"))
        .stdout(predicate::str::contains("ok install"));
    assert!(skills_dir.join("installable.md").exists());
    openai.assert_calls(1);
}

#[test]
fn install_skill_url_with_sha256_verification_works_end_to_end() {
    let server = MockServer::start();
    let remote_body = "Remote checksum skill";
    let checksum = format!("{:x}", Sha256::digest(remote_body.as_bytes()));

    let remote = server.mock(|when, then| {
        when.method(GET).path("/skills/remote.md");
        then.status(200).body(remote_body);
    });

    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .json_body_includes(
                json!({
                    "messages": [{
                        "role": "system",
                        "content": "base\n\n# Skill: remote\nRemote checksum skill"
                    }]
                })
                .to_string(),
            );
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "ok remote"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 7, "completion_tokens": 1, "total_tokens": 8}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--system-prompt",
        "base",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--install-skill-url",
        &format!("{}/skills/remote.md", server.base_url()),
        "--install-skill-sha256",
        &checksum,
        "--skill",
        "remote",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "remote skills install: installed=1",
        ))
        .stdout(predicate::str::contains("ok remote"));
    assert!(skills_dir.join("remote.md").exists());
    remote.assert_calls(1);
    openai.assert_calls(1);
}

#[test]
fn install_skill_from_registry_works_end_to_end() {
    let server = MockServer::start();
    let skill_body = "Registry-driven skill";
    let skill_sha = format!("{:x}", Sha256::digest(skill_body.as_bytes()));
    let registry_body = json!({
        "version": 1,
        "skills": [{
            "name": "reg",
            "url": format!("{}/skills/reg.md", server.base_url()),
            "sha256": skill_sha
        }]
    })
    .to_string();
    let registry_sha = format!("{:x}", Sha256::digest(registry_body.as_bytes()));

    let registry = server.mock(|when, then| {
        when.method(GET).path("/registry.json");
        then.status(200).body(registry_body);
    });
    let remote = server.mock(|when, then| {
        when.method(GET).path("/skills/reg.md");
        then.status(200).body(skill_body);
    });
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .json_body_includes(
                json!({
                    "messages": [{
                        "role": "system",
                        "content": "base\n\n# Skill: reg\nRegistry-driven skill"
                    }]
                })
                .to_string(),
            );
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "ok registry"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 8, "completion_tokens": 1, "total_tokens": 9}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--system-prompt",
        "base",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skill-registry-url",
        &format!("{}/registry.json", server.base_url()),
        "--skill-registry-sha256",
        &registry_sha,
        "--install-skill-from-registry",
        "reg",
        "--skill",
        "reg",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "registry skills install: installed=1",
        ))
        .stdout(predicate::str::contains("ok registry"));
    assert!(skills_dir.join("reg.md").exists());
    registry.assert_calls(1);
    remote.assert_calls(1);
    openai.assert_calls(1);
}

#[test]
fn install_signed_skill_from_registry_with_trust_root_works_end_to_end() {
    let server = MockServer::start();
    let root = SigningKey::from_bytes(&[41u8; 32]);
    let publisher = SigningKey::from_bytes(&[42u8; 32]);
    let root_public_key = BASE64.encode(root.verifying_key().to_bytes());
    let publisher_public_key = BASE64.encode(publisher.verifying_key().to_bytes());
    let publisher_certificate = BASE64.encode(
        root.sign(format!("pi-skill-key-v1:publisher:{publisher_public_key}").as_bytes())
            .to_bytes(),
    );

    let skill_body = "Signed registry skill";
    let skill_sha = format!("{:x}", Sha256::digest(skill_body.as_bytes()));
    let skill_signature = BASE64.encode(publisher.sign(skill_body.as_bytes()).to_bytes());
    let registry_body = json!({
        "version": 1,
        "keys": [{
            "id":"publisher",
            "public_key": publisher_public_key,
            "signed_by":"root",
            "signature": publisher_certificate
        }],
        "skills": [{
            "name": "reg-secure",
            "url": format!("{}/skills/reg-secure.md", server.base_url()),
            "sha256": skill_sha,
            "signing_key":"publisher",
            "signature": skill_signature
        }]
    })
    .to_string();
    let registry_sha = format!("{:x}", Sha256::digest(registry_body.as_bytes()));

    let registry = server.mock(|when, then| {
        when.method(GET).path("/registry.json");
        then.status(200).body(registry_body);
    });
    let remote = server.mock(|when, then| {
        when.method(GET).path("/skills/reg-secure.md");
        then.status(200).body(skill_body);
    });
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .json_body_includes(
                json!({
                    "messages": [{
                        "role": "system",
                        "content": "base\n\n# Skill: reg-secure\nSigned registry skill"
                    }]
                })
                .to_string(),
            );
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "ok signed registry"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 8, "completion_tokens": 1, "total_tokens": 9}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--system-prompt",
        "base",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skill-registry-url",
        &format!("{}/registry.json", server.base_url()),
        "--skill-registry-sha256",
        &registry_sha,
        "--skill-trust-root",
        &format!("root={root_public_key}"),
        "--require-signed-skills",
        "--install-skill-from-registry",
        "reg-secure",
        "--skill",
        "reg-secure",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "registry skills install: installed=1",
        ))
        .stdout(predicate::str::contains("ok signed registry"));
    assert!(skills_dir.join("reg-secure.md").exists());
    registry.assert_calls(1);
    remote.assert_calls(1);
    openai.assert_calls(1);
}

#[test]
fn require_signed_skills_rejects_unsigned_registry_entries() {
    let server = MockServer::start();
    let registry_body = json!({
        "version": 1,
        "skills": [{
            "name": "unsigned",
            "url": format!("{}/skills/unsigned.md", server.base_url())
        }]
    })
    .to_string();

    let registry = server.mock(|when, then| {
        when.method(GET).path("/registry.json");
        then.status(200).body(registry_body);
    });

    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skill-registry-url",
        &format!("{}/registry.json", server.base_url()),
        "--require-signed-skills",
        "--install-skill-from-registry",
        "unsigned",
        "--no-session",
    ]);

    cmd.assert().failure().stderr(predicate::str::contains(
        "unsigned but signatures are required",
    ));
    registry.assert_calls(1);
}
