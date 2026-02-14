use std::{
    fs,
    path::{Path, PathBuf},
};

use assert_cmd::Command;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{Signer, SigningKey};
use httpmock::prelude::*;
use predicates::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
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

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum RpcSchemaCompatMode {
    DispatchNdjson,
    ServeNdjson,
}

#[derive(Debug, Deserialize, PartialEq)]
struct RpcSchemaCompatFixture {
    schema_version: u32,
    name: String,
    mode: RpcSchemaCompatMode,
    input_lines: Vec<String>,
    expected_processed_lines: usize,
    expected_error_count: usize,
    expected_responses: Vec<Value>,
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
    Command::new(assert_cmd::cargo::cargo_bin!("tau-coding-agent"))
}

fn write_model_catalog(path: &std::path::Path, entries: serde_json::Value) {
    let payload = json!({
        "schema_version": 1,
        "entries": entries,
    });
    fs::write(path, format!("{payload}\n")).expect("write model catalog");
}

fn rpc_schema_compat_fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join("rpc-schema-compat")
        .join(name)
}

fn load_rpc_schema_compat_fixture(name: &str) -> RpcSchemaCompatFixture {
    let path = rpc_schema_compat_fixture_path(name);
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    let fixture = serde_json::from_str::<RpcSchemaCompatFixture>(&raw)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()));
    assert_eq!(
        fixture.schema_version,
        1,
        "unsupported fixture schema_version in {}",
        path.display()
    );
    fixture
}

fn parse_ndjson_values(raw: &str) -> Vec<Value> {
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<Value>(line).expect("line should parse as JSON"))
        .collect::<Vec<_>>()
}

#[path = "cli_integration/auth_provider.rs"]
mod auth_provider;

#[path = "cli_integration/session_runtime.rs"]
mod session_runtime;

#[path = "cli_integration/bridge_transports.rs"]
mod bridge_transports;

#[path = "cli_integration/tooling_skills.rs"]
mod tooling_skills;

#[path = "cli_integration/orchestrator_harness.rs"]
mod orchestrator_harness;
