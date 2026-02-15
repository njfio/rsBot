//! Runtime helpers for building and registering generated wasm tools.
//!
//! This module implements a deterministic tool-builder flow:
//! spec -> wat source -> compile/retry -> persist -> sandbox validate.

use std::{
    fmt,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    execute_wasm_sandbox_sync, WasmSandboxCapabilityProfile, WasmSandboxExecutionRequest,
    WasmSandboxFilesystemMode, WasmSandboxLimits, WasmSandboxNetworkMode,
    WASM_SANDBOX_FUEL_LIMIT_DEFAULT, WASM_SANDBOX_MAX_RESPONSE_BYTES_DEFAULT,
    WASM_SANDBOX_MEMORY_LIMIT_BYTES_DEFAULT, WASM_SANDBOX_TIMEOUT_MS_DEFAULT,
};

const GENERATED_TOOL_SCHEMA_VERSION: u32 = 1;
const GENERATED_TOOL_MANIFEST_SCHEMA_VERSION: u32 = 1;
const GENERATED_TOOL_MANIFEST_VERSION: &str = "1.0.0";
const GENERATED_TOOL_MAX_ATTEMPTS_DEFAULT: usize = 3;
const GENERATED_TOOL_MAX_ATTEMPTS_MAX: usize = 8;
const GENERATED_TOOL_MODULE_FILENAME: &str = "tool.wasm";
const GENERATED_TOOL_SOURCE_FILENAME: &str = "tool.wat";
const GENERATED_TOOL_MANIFEST_FILENAME: &str = "extension.json";
const GENERATED_TOOL_METADATA_FILENAME: &str = "metadata.json";
const GENERATED_TOOL_REASON_COMPILE_SUCCESS: &str = "generated_tool_compile_succeeded";
const GENERATED_TOOL_REASON_COMPILE_FAILED: &str = "generated_tool_compile_failed";
const GENERATED_TOOL_REASON_PERSIST_SUCCESS: &str = "generated_tool_persist_succeeded";
const GENERATED_TOOL_REASON_REGISTER_SUCCESS: &str = "generated_tool_registered";
const GENERATED_TOOL_REASON_SANDBOX_VALIDATED: &str = "generated_tool_sandbox_validation_succeeded";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Build request for a generated wasm tool artifact.
pub struct GeneratedToolBuildRequest {
    pub tool_name: String,
    pub description: String,
    pub spec: String,
    pub parameters: Value,
    pub output_root: PathBuf,
    pub extension_root: PathBuf,
    pub max_attempts: usize,
    pub timeout_ms: u64,
    pub wasm_limits: WasmSandboxLimits,
    pub wasm_capabilities: WasmSandboxCapabilityProfile,
    pub provided_wat_source: Option<String>,
}

impl Default for GeneratedToolBuildRequest {
    fn default() -> Self {
        Self {
            tool_name: "generated_tool".to_string(),
            description: "Generated tool".to_string(),
            spec: "Summarize input".to_string(),
            parameters: json!({"type":"object","properties":{},"additionalProperties":false}),
            output_root: PathBuf::from(".tau/generated-tools"),
            extension_root: PathBuf::from(".tau/extensions/generated"),
            max_attempts: GENERATED_TOOL_MAX_ATTEMPTS_DEFAULT,
            timeout_ms: WASM_SANDBOX_TIMEOUT_MS_DEFAULT,
            wasm_limits: WasmSandboxLimits {
                fuel_limit: WASM_SANDBOX_FUEL_LIMIT_DEFAULT,
                memory_limit_bytes: WASM_SANDBOX_MEMORY_LIMIT_BYTES_DEFAULT,
                timeout_ms: WASM_SANDBOX_TIMEOUT_MS_DEFAULT,
                max_response_bytes: WASM_SANDBOX_MAX_RESPONSE_BYTES_DEFAULT,
            },
            wasm_capabilities: WasmSandboxCapabilityProfile {
                filesystem_mode: WasmSandboxFilesystemMode::Deny,
                network_mode: WasmSandboxNetworkMode::Deny,
                env_allowlist: Vec::new(),
            },
            provided_wat_source: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Attempt-level build record emitted by the compile/retry loop.
pub struct GeneratedToolBuildAttempt {
    pub attempt: u32,
    pub status: String,
    pub reason_code: String,
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Build report persisted and returned to callers.
pub struct GeneratedToolBuildReport {
    pub schema_version: u32,
    pub tool_name: String,
    pub manifest_id: String,
    pub manifest_path: PathBuf,
    pub module_path: PathBuf,
    pub source_path: PathBuf,
    pub metadata_path: PathBuf,
    pub attempts: Vec<GeneratedToolBuildAttempt>,
    pub reason_codes: Vec<String>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Structured failure for generated tool build operations.
pub struct GeneratedToolBuildError {
    pub reason_code: String,
    pub message: String,
    pub diagnostics: Vec<String>,
}

impl GeneratedToolBuildError {
    fn new(reason_code: &str, message: impl Into<String>) -> Self {
        Self {
            reason_code: reason_code.to_string(),
            message: message.into(),
            diagnostics: Vec::new(),
        }
    }

    fn with_diagnostics(
        reason_code: &str,
        message: impl Into<String>,
        diagnostics: Vec<String>,
    ) -> Self {
        Self {
            reason_code: reason_code.to_string(),
            message: message.into(),
            diagnostics,
        }
    }
}

impl fmt::Display for GeneratedToolBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.diagnostics.is_empty() {
            write!(f, "{} ({})", self.message, self.reason_code)
        } else {
            write!(
                f,
                "{} ({}) diagnostics={}",
                self.message,
                self.reason_code,
                self.diagnostics.join("; ")
            )
        }
    }
}

impl std::error::Error for GeneratedToolBuildError {}

/// Builds and registers a generated wasm tool artifact with retry and validation.
pub fn build_generated_wasm_tool(
    request: GeneratedToolBuildRequest,
) -> Result<GeneratedToolBuildReport, GeneratedToolBuildError> {
    let tool_name = normalize_tool_name(&request.tool_name)?;
    validate_build_request(&request)?;

    std::fs::create_dir_all(&request.output_root).map_err(|error| {
        GeneratedToolBuildError::new(
            "generated_tool_output_root_create_failed",
            format!(
                "failed to create generated tool output root '{}': {error}",
                request.output_root.display()
            ),
        )
    })?;
    std::fs::create_dir_all(&request.extension_root).map_err(|error| {
        GeneratedToolBuildError::new(
            "generated_tool_extension_root_create_failed",
            format!(
                "failed to create generated tool extension root '{}': {error}",
                request.extension_root.display()
            ),
        )
    })?;

    let artifact_dir = request.output_root.join(&tool_name);
    let extension_dir = request.extension_root.join(&tool_name);
    std::fs::create_dir_all(&artifact_dir).map_err(|error| {
        GeneratedToolBuildError::new(
            "generated_tool_artifact_dir_create_failed",
            format!(
                "failed to create generated tool artifact directory '{}': {error}",
                artifact_dir.display()
            ),
        )
    })?;
    std::fs::create_dir_all(&extension_dir).map_err(|error| {
        GeneratedToolBuildError::new(
            "generated_tool_extension_dir_create_failed",
            format!(
                "failed to create generated tool extension directory '{}': {error}",
                extension_dir.display()
            ),
        )
    })?;

    let source_path = artifact_dir.join(GENERATED_TOOL_SOURCE_FILENAME);
    let metadata_path = artifact_dir.join(GENERATED_TOOL_METADATA_FILENAME);
    let module_path = extension_dir.join(GENERATED_TOOL_MODULE_FILENAME);
    let manifest_path = extension_dir.join(GENERATED_TOOL_MANIFEST_FILENAME);
    let manifest_id = format!("generated-tool-{}", tool_name);

    let max_attempts = request
        .max_attempts
        .clamp(1, GENERATED_TOOL_MAX_ATTEMPTS_MAX);
    let mut attempts = Vec::new();
    let mut wat_source = request
        .provided_wat_source
        .clone()
        .unwrap_or_else(|| synthesize_wat_source(&tool_name, &request.spec, None));
    let mut module_bytes = None;
    for attempt in 1..=max_attempts {
        match wat::parse_str(&wat_source) {
            Ok(bytes) => {
                attempts.push(GeneratedToolBuildAttempt {
                    attempt: attempt as u32,
                    status: "succeeded".to_string(),
                    reason_code: GENERATED_TOOL_REASON_COMPILE_SUCCESS.to_string(),
                    diagnostic: None,
                });
                module_bytes = Some(bytes);
                break;
            }
            Err(error) => {
                let diagnostic = error.to_string();
                attempts.push(GeneratedToolBuildAttempt {
                    attempt: attempt as u32,
                    status: "failed".to_string(),
                    reason_code: GENERATED_TOOL_REASON_COMPILE_FAILED.to_string(),
                    diagnostic: Some(diagnostic.clone()),
                });
                if attempt == max_attempts {
                    return Err(GeneratedToolBuildError::with_diagnostics(
                        "generated_tool_compile_max_attempts_exceeded",
                        format!(
                            "failed to compile generated tool '{}' after {} attempt(s)",
                            tool_name, max_attempts
                        ),
                        attempts
                            .iter()
                            .filter_map(|attempt| {
                                attempt.diagnostic.as_ref().map(|diagnostic| {
                                    format!("attempt{}: {}", attempt.attempt, diagnostic)
                                })
                            })
                            .collect(),
                    ));
                }
                wat_source = synthesize_wat_source(&tool_name, &request.spec, Some(&diagnostic));
            }
        }
    }
    let module_bytes = module_bytes.ok_or_else(|| {
        GeneratedToolBuildError::new(
            "generated_tool_compile_internal_missing_output",
            "generated tool compiler exited without output bytes",
        )
    })?;

    std::fs::write(&source_path, &wat_source).map_err(|error| {
        GeneratedToolBuildError::new(
            "generated_tool_source_write_failed",
            format!(
                "failed to persist generated wat source '{}': {error}",
                source_path.display()
            ),
        )
    })?;
    std::fs::write(&module_path, module_bytes).map_err(|error| {
        GeneratedToolBuildError::new(
            "generated_tool_module_write_failed",
            format!(
                "failed to persist generated wasm module '{}': {error}",
                module_path.display()
            ),
        )
    })?;

    let manifest_json = build_generated_manifest_json(
        &tool_name,
        &manifest_id,
        &request.description,
        &request.parameters,
        request.timeout_ms.max(1),
        &request.wasm_limits,
        &request.wasm_capabilities,
    );
    let manifest_bytes = serde_json::to_vec_pretty(&manifest_json).map_err(|error| {
        GeneratedToolBuildError::new(
            "generated_tool_manifest_serialize_failed",
            format!("failed to serialize generated tool manifest JSON: {error}"),
        )
    })?;
    std::fs::write(&manifest_path, manifest_bytes).map_err(|error| {
        GeneratedToolBuildError::new(
            "generated_tool_manifest_write_failed",
            format!(
                "failed to persist generated tool manifest '{}': {error}",
                manifest_path.display()
            ),
        )
    })?;

    validate_generated_module_in_sandbox(
        &module_path,
        &manifest_id,
        &tool_name,
        &request.wasm_limits,
        &request.wasm_capabilities,
    )?;

    let mut reason_codes = vec![
        GENERATED_TOOL_REASON_COMPILE_SUCCESS.to_string(),
        GENERATED_TOOL_REASON_PERSIST_SUCCESS.to_string(),
        GENERATED_TOOL_REASON_REGISTER_SUCCESS.to_string(),
        GENERATED_TOOL_REASON_SANDBOX_VALIDATED.to_string(),
    ];
    if attempts
        .iter()
        .any(|attempt| attempt.reason_code == GENERATED_TOOL_REASON_COMPILE_FAILED)
    {
        reason_codes.insert(0, GENERATED_TOOL_REASON_COMPILE_FAILED.to_string());
    }
    let diagnostics = vec![
        format!("tool={tool_name}"),
        format!("module={}", module_path.display()),
        format!("manifest={}", manifest_path.display()),
    ];

    let report = GeneratedToolBuildReport {
        schema_version: GENERATED_TOOL_SCHEMA_VERSION,
        tool_name,
        manifest_id,
        manifest_path,
        module_path,
        source_path,
        metadata_path: metadata_path.clone(),
        attempts,
        reason_codes,
        diagnostics,
    };
    let metadata_bytes = serde_json::to_vec_pretty(&report).map_err(|error| {
        GeneratedToolBuildError::new(
            "generated_tool_metadata_serialize_failed",
            format!("failed to serialize generated tool metadata JSON: {error}"),
        )
    })?;
    std::fs::write(&metadata_path, metadata_bytes).map_err(|error| {
        GeneratedToolBuildError::new(
            "generated_tool_metadata_write_failed",
            format!(
                "failed to persist generated tool metadata '{}': {error}",
                metadata_path.display()
            ),
        )
    })?;

    Ok(report)
}

fn validate_build_request(
    request: &GeneratedToolBuildRequest,
) -> Result<(), GeneratedToolBuildError> {
    if request.description.trim().is_empty() {
        return Err(GeneratedToolBuildError::new(
            "generated_tool_description_empty",
            "generated tool description must not be empty",
        ));
    }
    if request.spec.trim().is_empty() {
        return Err(GeneratedToolBuildError::new(
            "generated_tool_spec_empty",
            "generated tool spec must not be empty",
        ));
    }
    if !request.parameters.is_object() {
        return Err(GeneratedToolBuildError::new(
            "generated_tool_parameters_invalid",
            "generated tool parameters must be a JSON object",
        ));
    }
    Ok(())
}

fn normalize_tool_name(raw: &str) -> Result<String, GeneratedToolBuildError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(GeneratedToolBuildError::new(
            "generated_tool_name_empty",
            "generated tool name must not be empty",
        ));
    }
    if !trimmed.chars().all(|character| {
        character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || character == '-'
            || character == '_'
    }) {
        return Err(GeneratedToolBuildError::new(
            "generated_tool_name_invalid",
            "generated tool name must contain only lowercase alphanumeric, dash, or underscore characters",
        ));
    }
    Ok(trimmed.to_string())
}

fn synthesize_wat_source(tool_name: &str, spec: &str, feedback: Option<&str>) -> String {
    let summary = truncate_text(spec.trim(), 160);
    let detail = feedback
        .map(|feedback| {
            format!(
                "retry after compile error: {}",
                truncate_text(feedback, 120)
            )
        })
        .unwrap_or_else(|| "first-pass generated module".to_string());
    let response_json = json!({
        "content": {
            "status": "ok",
            "tool": tool_name,
            "summary": summary,
            "detail": detail,
        },
        "is_error": false
    })
    .to_string();
    let escaped = response_json.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        r#"(module
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 1024))
  (data (i32.const 0) "{escaped}")
  (func (export "tau_extension_alloc") (param $len i32) (result i32)
    (local $ptr i32)
    global.get $heap
    local.set $ptr
    global.get $heap
    local.get $len
    i32.add
    global.set $heap
    local.get $ptr)
  (func (export "tau_extension_invoke") (param i32 i32) (result i64)
    i64.const {response_len})
)"#,
        response_len = response_json.len()
    )
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn build_generated_manifest_json(
    tool_name: &str,
    manifest_id: &str,
    description: &str,
    parameters: &Value,
    timeout_ms: u64,
    wasm_limits: &WasmSandboxLimits,
    wasm_capabilities: &WasmSandboxCapabilityProfile,
) -> Value {
    json!({
        "schema_version": GENERATED_TOOL_MANIFEST_SCHEMA_VERSION,
        "id": manifest_id,
        "version": GENERATED_TOOL_MANIFEST_VERSION,
        "runtime": "wasm",
        "entrypoint": GENERATED_TOOL_MODULE_FILENAME,
        "permissions": ["run-commands"],
        "tools": [{
            "name": tool_name,
            "description": description.trim(),
            "parameters": parameters,
        }],
        "timeout_ms": timeout_ms,
        "wasm": {
            "fuel_limit": wasm_limits.fuel_limit,
            "memory_limit_bytes": wasm_limits.memory_limit_bytes,
            "max_response_bytes": wasm_limits.max_response_bytes,
            "filesystem_mode": wasm_capability_filesystem_mode_name(wasm_capabilities.filesystem_mode),
            "network_mode": wasm_capability_network_mode_name(wasm_capabilities.network_mode),
            "env_allowlist": wasm_capabilities.env_allowlist,
        }
    })
}

fn wasm_capability_filesystem_mode_name(mode: WasmSandboxFilesystemMode) -> &'static str {
    match mode {
        WasmSandboxFilesystemMode::Deny => "deny",
        WasmSandboxFilesystemMode::ReadOnly => "read-only",
        WasmSandboxFilesystemMode::ReadWrite => "read-write",
    }
}

fn wasm_capability_network_mode_name(mode: WasmSandboxNetworkMode) -> &'static str {
    match mode {
        WasmSandboxNetworkMode::Deny => "deny",
        WasmSandboxNetworkMode::Allow => "allow",
    }
}

fn validate_generated_module_in_sandbox(
    module_path: &Path,
    manifest_id: &str,
    tool_name: &str,
    wasm_limits: &WasmSandboxLimits,
    wasm_capabilities: &WasmSandboxCapabilityProfile,
) -> Result<(), GeneratedToolBuildError> {
    let request_json = serde_json::to_string(&json!({
        "hook": "tool-call",
        "payload": {
            "schema_version": 1,
            "kind": "tool-call",
            "tool": {
                "name": tool_name,
                "arguments": {},
            },
        },
        "manifest_id": manifest_id,
        "manifest_version": GENERATED_TOOL_MANIFEST_VERSION,
    }))
    .map_err(|error| {
        GeneratedToolBuildError::new(
            "generated_tool_validation_request_serialize_failed",
            format!("failed to serialize generated tool sandbox validation request: {error}"),
        )
    })?;

    let report = execute_wasm_sandbox_sync(WasmSandboxExecutionRequest {
        module_path: module_path.to_path_buf(),
        request_json,
        limits: wasm_limits.clone(),
        capabilities: wasm_capabilities.clone(),
    })
    .map_err(|error| {
        GeneratedToolBuildError::with_diagnostics(
            "generated_tool_sandbox_validation_failed",
            format!(
                "generated tool sandbox validation failed with reason_code={}",
                error.reason_code
            ),
            if error.diagnostics.is_empty() {
                vec![error.message]
            } else {
                error.diagnostics
            },
        )
    })?;

    let response_json = serde_json::from_str::<Value>(&report.response_json).map_err(|error| {
        GeneratedToolBuildError::new(
            "generated_tool_validation_response_invalid",
            format!("generated tool sandbox validation response is not valid JSON: {error}"),
        )
    })?;
    let response_object = response_json.as_object().ok_or_else(|| {
        GeneratedToolBuildError::new(
            "generated_tool_validation_response_invalid",
            "generated tool sandbox validation response must be a JSON object",
        )
    })?;
    if !response_object.contains_key("content") {
        return Err(GeneratedToolBuildError::new(
            "generated_tool_validation_response_missing_content",
            "generated tool sandbox validation response must include 'content'",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        build_generated_wasm_tool, synthesize_wat_source, GeneratedToolBuildRequest,
        GENERATED_TOOL_REASON_COMPILE_FAILED, GENERATED_TOOL_REASON_COMPILE_SUCCESS,
        GENERATED_TOOL_REASON_SANDBOX_VALIDATED,
    };
    use tempfile::tempdir;

    #[test]
    fn unit_synthesize_wat_source_produces_compilable_module() {
        let wat_source = synthesize_wat_source("issue_triage", "summarize pending issue", None);
        let bytes = wat::parse_str(&wat_source).expect("wat should compile");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn functional_build_generated_wasm_tool_persists_artifacts_and_manifest() {
        let temp = tempdir().expect("tempdir");
        let output_root = temp.path().join("generated-tools");
        let extension_root = temp.path().join("extensions");
        let report = build_generated_wasm_tool(GeneratedToolBuildRequest {
            tool_name: "issue_triage".to_string(),
            description: "Generated issue triage tool".to_string(),
            spec: "Return structured triage recommendation".to_string(),
            output_root,
            extension_root,
            ..GeneratedToolBuildRequest::default()
        })
        .expect("generated tool build should succeed");

        assert!(report.module_path.is_file());
        assert!(report.manifest_path.is_file());
        assert!(report.metadata_path.is_file());
        assert!(report
            .reason_codes
            .iter()
            .any(|reason| reason == GENERATED_TOOL_REASON_SANDBOX_VALIDATED));
    }

    #[test]
    fn integration_build_generated_wasm_tool_retries_invalid_seed_wat_then_succeeds() {
        let temp = tempdir().expect("tempdir");
        let output_root = temp.path().join("generated-tools");
        let extension_root = temp.path().join("extensions");
        let report = build_generated_wasm_tool(GeneratedToolBuildRequest {
            tool_name: "issue_triage".to_string(),
            description: "Generated issue triage tool".to_string(),
            spec: "Return structured triage recommendation".to_string(),
            output_root,
            extension_root,
            max_attempts: 3,
            provided_wat_source: Some("(module".to_string()),
            ..GeneratedToolBuildRequest::default()
        })
        .expect("generated tool build should recover from invalid seed wat");

        assert!(report.attempts.len() >= 2);
        assert_eq!(
            report.attempts[0].reason_code,
            GENERATED_TOOL_REASON_COMPILE_FAILED
        );
        assert_eq!(
            report
                .attempts
                .last()
                .map(|attempt| attempt.reason_code.as_str()),
            Some(GENERATED_TOOL_REASON_COMPILE_SUCCESS)
        );
    }

    #[test]
    fn regression_build_generated_wasm_tool_fails_closed_on_non_object_parameters() {
        let temp = tempdir().expect("tempdir");
        let error = build_generated_wasm_tool(GeneratedToolBuildRequest {
            tool_name: "issue_triage".to_string(),
            description: "Generated issue triage tool".to_string(),
            spec: "Return structured triage recommendation".to_string(),
            output_root: temp.path().join("generated-tools"),
            extension_root: temp.path().join("extensions"),
            parameters: serde_json::json!("not-an-object"),
            ..GeneratedToolBuildRequest::default()
        })
        .expect_err("non-object parameters should fail");

        assert_eq!(error.reason_code, "generated_tool_parameters_invalid");
    }

    #[test]
    fn regression_build_generated_wasm_tool_fails_on_invalid_name() {
        let temp = tempdir().expect("tempdir");
        let error = build_generated_wasm_tool(GeneratedToolBuildRequest {
            tool_name: "Issue-Triage".to_string(),
            description: "Generated issue triage tool".to_string(),
            spec: "Return structured triage recommendation".to_string(),
            output_root: temp.path().join("generated-tools"),
            extension_root: temp.path().join("extensions"),
            ..GeneratedToolBuildRequest::default()
        })
        .expect_err("invalid tool name should fail");

        assert_eq!(error.reason_code, "generated_tool_name_invalid");
    }
}
