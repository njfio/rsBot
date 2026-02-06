use std::{fs, process::Command};

use assert_cmd::assert::OutputAssertExt;
use httpmock::prelude::*;
use predicates::prelude::*;
use serde::Deserialize;
use serde_json::json;
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

    openai.assert_hits(2);
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

    anthropic.assert_hits(1);
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

    google.assert_hits(1);
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

    openai.assert_hits(1);
}

#[test]
fn selected_skill_is_included_in_system_prompt() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .json_body_partial(
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
    openai.assert_hits(1);
}

#[test]
fn install_skill_flag_installs_skill_before_prompt() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .json_body_partial(
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
    openai.assert_hits(1);
}
