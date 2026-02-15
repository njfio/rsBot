//! Extension manifest, command execution, and runtime hook support for Tau.
//!
//! Provides extension discovery/validation and runtime-hook dispatch used to
//! customize runtime behavior and command surfaces for process and wasm modules.

use std::{
    collections::HashSet,
    fs,
    hash::Hash,
    io::Write,
    path::{Component, Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tau_cli::Cli;
use tau_runtime::{
    execute_wasm_sandbox_sync, WasmSandboxCapabilityProfile, WasmSandboxExecutionRequest,
    WasmSandboxFilesystemMode, WasmSandboxLimits, WasmSandboxNetworkMode,
    WASM_SANDBOX_FUEL_LIMIT_DEFAULT, WASM_SANDBOX_MAX_RESPONSE_BYTES_DEFAULT,
    WASM_SANDBOX_MEMORY_LIMIT_BYTES_DEFAULT,
};
use wait_timeout::ChildExt;

#[cfg(test)]
use std::sync::{Mutex, OnceLock};

const EXTENSION_MANIFEST_SCHEMA_VERSION: u32 = 1;
const EXTENSION_TIMEOUT_MS_DEFAULT: u64 = 5_000;
const EXTENSION_TIMEOUT_MS_MAX: u64 = 300_000;
const EXTENSION_HOOK_PAYLOAD_SCHEMA_VERSION: u32 = 1;
const EXTENSION_COMMAND_RESPONSE_ACTION_CONTINUE: &str = "continue";
const EXTENSION_COMMAND_RESPONSE_ACTION_EXIT: &str = "exit";
const EXTENSION_PROCESS_EXECUTION_SUCCEEDED_REASON_CODE: &str = "process_execution_succeeded";

pub fn execute_extension_list_command(cli: &Cli) -> Result<()> {
    if !cli.extension_list {
        return Ok(());
    }
    let report = list_extension_manifests(&cli.extension_list_root)?;
    println!("{}", render_extension_list_report(&report));
    Ok(())
}

pub fn execute_extension_exec_command(cli: &Cli) -> Result<()> {
    let Some(manifest_path) = cli.extension_exec_manifest.as_ref() else {
        return Ok(());
    };
    let hook = cli
        .extension_exec_hook
        .as_deref()
        .ok_or_else(|| anyhow!("--extension-exec-hook is required"))?;
    let payload_file = cli
        .extension_exec_payload_file
        .as_ref()
        .ok_or_else(|| anyhow!("--extension-exec-payload-file is required"))?;
    let payload = load_extension_exec_payload(payload_file)?;
    let summary = execute_extension_process_hook(manifest_path, hook, &payload)?;
    println!(
        "extension exec: path={} id={} version={} runtime={} hook={} timeout_ms={} duration_ms={} response_bytes={} reason_codes={} diagnostics={}",
        summary.manifest_path.display(),
        summary.id,
        summary.version,
        summary.runtime,
        summary.hook,
        summary.timeout_ms,
        summary.duration_ms,
        summary.response_bytes,
        summary.reason_codes.join(","),
        summary.diagnostics.len()
    );
    println!("extension exec response: {}", summary.response);
    Ok(())
}

pub fn execute_extension_show_command(cli: &Cli) -> Result<()> {
    let Some(path) = cli.extension_show.as_ref() else {
        return Ok(());
    };
    let (manifest, summary) = load_and_validate_extension_manifest(path)?;
    println!("{}", render_extension_manifest_report(&summary, &manifest));
    Ok(())
}

pub fn execute_extension_validate_command(cli: &Cli) -> Result<()> {
    let Some(path) = cli.extension_validate.as_ref() else {
        return Ok(());
    };
    let summary = validate_extension_manifest(path)?;
    println!(
        "extension validate: path={} id={} version={} runtime={} entrypoint={} hooks={} permissions={} timeout_ms={}",
        summary.manifest_path.display(),
        summary.id,
        summary.version,
        summary.runtime,
        summary.entrypoint,
        summary.hook_count,
        summary.permission_count,
        summary.timeout_ms
    );
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `ExtensionManifestSummary` used across Tau components.
pub struct ExtensionManifestSummary {
    pub manifest_path: PathBuf,
    pub id: String,
    pub version: String,
    pub runtime: String,
    pub entrypoint: String,
    pub hook_count: usize,
    pub permission_count: usize,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `ExtensionListEntry` used across Tau components.
pub struct ExtensionListEntry {
    pub manifest_path: PathBuf,
    pub id: String,
    pub version: String,
    pub runtime: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `ExtensionListInvalidEntry` used across Tau components.
pub struct ExtensionListInvalidEntry {
    pub manifest_path: PathBuf,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `ExtensionListReport` used across Tau components.
pub struct ExtensionListReport {
    pub list_root: PathBuf,
    pub entries: Vec<ExtensionListEntry>,
    pub invalid_entries: Vec<ExtensionListInvalidEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `ExtensionExecSummary` used across Tau components.
pub struct ExtensionExecSummary {
    pub manifest_path: PathBuf,
    pub id: String,
    pub version: String,
    pub runtime: String,
    pub hook: String,
    pub timeout_ms: u64,
    pub duration_ms: u64,
    pub response_bytes: usize,
    pub response: String,
    pub reason_codes: Vec<String>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `ExtensionRuntimeHookDispatchSummary` used across Tau components.
pub struct ExtensionRuntimeHookDispatchSummary {
    pub root: PathBuf,
    pub hook: String,
    pub discovered: usize,
    pub eligible: usize,
    pub executed: usize,
    pub failed: usize,
    pub skipped_invalid: usize,
    pub skipped_unsupported_runtime: usize,
    pub skipped_undeclared_hook: usize,
    pub skipped_permission_denied: usize,
    pub executed_ids: Vec<String>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `ExtensionMessageTransformResult` used across Tau components.
pub struct ExtensionMessageTransformResult {
    pub root: PathBuf,
    pub prompt: String,
    pub executed: usize,
    pub applied: usize,
    pub failed: usize,
    pub skipped_invalid: usize,
    pub skipped_unsupported_runtime: usize,
    pub skipped_undeclared_hook: usize,
    pub skipped_permission_denied: usize,
    pub applied_ids: Vec<String>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `ExtensionPolicyOverrideResult` used across Tau components.
pub struct ExtensionPolicyOverrideResult {
    pub root: PathBuf,
    pub allowed: bool,
    pub denied_by: Option<String>,
    pub reason: Option<String>,
    pub evaluated: usize,
    pub denied: usize,
    pub permission_denied: usize,
    pub skipped_invalid: usize,
    pub skipped_unsupported_runtime: usize,
    pub skipped_undeclared_hook: usize,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Enumerates supported `ExtensionRegisteredCommandAction` values.
pub enum ExtensionRegisteredCommandAction {
    Continue,
    Exit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `ExtensionRegisteredCommandResult` used across Tau components.
pub struct ExtensionRegisteredCommandResult {
    pub output: Option<String>,
    pub action: ExtensionRegisteredCommandAction,
}

#[derive(Debug, Clone, PartialEq)]
/// Public struct `ExtensionRegisteredToolResult` used across Tau components.
pub struct ExtensionRegisteredToolResult {
    pub content: Value,
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq)]
/// Public struct `ExtensionRegisteredTool` used across Tau components.
pub struct ExtensionRegisteredTool {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    pub runtime: String,
    pub extension_id: String,
    pub extension_version: String,
    pub manifest_path: PathBuf,
    pub entrypoint: PathBuf,
    pub timeout_ms: u64,
    pub wasm_limits: Option<WasmSandboxLimits>,
    pub wasm_capabilities: Option<WasmSandboxCapabilityProfile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `ExtensionRegisteredCommand` used across Tau components.
pub struct ExtensionRegisteredCommand {
    pub name: String,
    pub description: String,
    pub usage: Option<String>,
    pub runtime: String,
    pub extension_id: String,
    pub extension_version: String,
    pub manifest_path: PathBuf,
    pub entrypoint: PathBuf,
    pub timeout_ms: u64,
    pub wasm_limits: Option<WasmSandboxLimits>,
    pub wasm_capabilities: Option<WasmSandboxCapabilityProfile>,
}

#[derive(Debug, Clone, PartialEq)]
/// Public struct `ExtensionRuntimeRegistrationSummary` used across Tau components.
pub struct ExtensionRuntimeRegistrationSummary {
    pub root: PathBuf,
    pub discovered: usize,
    pub registered_tools: Vec<ExtensionRegisteredTool>,
    pub registered_commands: Vec<ExtensionRegisteredCommand>,
    pub skipped_invalid: usize,
    pub skipped_unsupported_runtime: usize,
    pub skipped_permission_denied: usize,
    pub skipped_name_conflict: usize,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PolicyOverrideDecision {
    Allow,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PolicyOverrideResponse {
    decision: PolicyOverrideDecision,
    reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Public struct `ExtensionManifest` used across Tau components.
pub struct ExtensionManifest {
    schema_version: u32,
    id: String,
    version: String,
    runtime: ExtensionRuntime,
    entrypoint: String,
    #[serde(default)]
    hooks: Vec<ExtensionHook>,
    #[serde(default)]
    permissions: Vec<ExtensionPermission>,
    #[serde(default)]
    tools: Vec<ExtensionToolRegistration>,
    #[serde(default)]
    commands: Vec<ExtensionCommandRegistration>,
    #[serde(default = "default_extension_timeout_ms")]
    timeout_ms: u64,
    #[serde(default)]
    wasm: ExtensionWasmRuntimeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
struct ExtensionWasmRuntimeConfig {
    #[serde(default)]
    fuel_limit: Option<u64>,
    #[serde(default)]
    memory_limit_bytes: Option<u64>,
    #[serde(default)]
    max_response_bytes: Option<usize>,
    #[serde(default)]
    filesystem_mode: Option<ExtensionWasmFilesystemMode>,
    #[serde(default)]
    network_mode: Option<ExtensionWasmNetworkMode>,
    #[serde(default)]
    env_allowlist: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum ExtensionWasmFilesystemMode {
    Deny,
    ReadOnly,
    ReadWrite,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum ExtensionWasmNetworkMode {
    Deny,
    Allow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExtensionToolRegistration {
    name: String,
    description: String,
    parameters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExtensionCommandRegistration {
    name: String,
    description: String,
    #[serde(default)]
    usage: Option<String>,
}

struct LoadedExtensionManifest {
    manifest: ExtensionManifest,
    summary: ExtensionManifestSummary,
}

fn default_extension_timeout_ms() -> u64 {
    EXTENSION_TIMEOUT_MS_DEFAULT
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum ExtensionRuntime {
    Process,
    Wasm,
}

impl ExtensionRuntime {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Process => "process",
            Self::Wasm => "wasm",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
enum ExtensionHook {
    RunStart,
    RunEnd,
    PreToolCall,
    PostToolCall,
    MessageTransform,
    PolicyOverride,
}

impl ExtensionHook {
    fn as_str(&self) -> &'static str {
        match self {
            Self::RunStart => "run-start",
            Self::RunEnd => "run-end",
            Self::PreToolCall => "pre-tool-call",
            Self::PostToolCall => "post-tool-call",
            Self::MessageTransform => "message-transform",
            Self::PolicyOverride => "policy-override",
        }
    }

    fn parse(raw: &str) -> Result<Self> {
        match raw.trim() {
            "run-start" => Ok(Self::RunStart),
            "run-end" => Ok(Self::RunEnd),
            "pre-tool-call" => Ok(Self::PreToolCall),
            "post-tool-call" => Ok(Self::PostToolCall),
            "message-transform" => Ok(Self::MessageTransform),
            "policy-override" => Ok(Self::PolicyOverride),
            other => bail!(
                "unsupported extension hook '{}': expected one of run-start, run-end, pre-tool-call, post-tool-call, message-transform, policy-override",
                other
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
enum ExtensionPermission {
    ReadFiles,
    WriteFiles,
    RunCommands,
    Network,
}

impl ExtensionPermission {
    fn as_str(&self) -> &'static str {
        match self {
            Self::ReadFiles => "read-files",
            Self::WriteFiles => "write-files",
            Self::RunCommands => "run-commands",
            Self::Network => "network",
        }
    }
}

fn required_permission_for_hook(hook: &ExtensionHook) -> Option<ExtensionPermission> {
    match hook {
        ExtensionHook::RunStart
        | ExtensionHook::RunEnd
        | ExtensionHook::PreToolCall
        | ExtensionHook::PostToolCall
        | ExtensionHook::MessageTransform
        | ExtensionHook::PolicyOverride => Some(ExtensionPermission::RunCommands),
    }
}

#[derive(Debug, Clone)]
struct ExtensionHookExecutionOutput {
    response: String,
    response_bytes: usize,
    duration_ms: u64,
    reason_codes: Vec<String>,
    diagnostics: Vec<String>,
}

fn is_supported_extension_runtime(runtime: &ExtensionRuntime) -> bool {
    matches!(runtime, ExtensionRuntime::Process | ExtensionRuntime::Wasm)
}

fn parse_registered_extension_runtime(runtime: &str) -> Result<ExtensionRuntime> {
    match runtime.trim() {
        "process" => Ok(ExtensionRuntime::Process),
        "wasm" => Ok(ExtensionRuntime::Wasm),
        other => bail!("unsupported extension runtime '{}'", other),
    }
}

fn wasm_runtime_limits_from_manifest(manifest: &ExtensionManifest) -> WasmSandboxLimits {
    WasmSandboxLimits {
        fuel_limit: manifest
            .wasm
            .fuel_limit
            .unwrap_or(WASM_SANDBOX_FUEL_LIMIT_DEFAULT),
        memory_limit_bytes: manifest
            .wasm
            .memory_limit_bytes
            .unwrap_or(WASM_SANDBOX_MEMORY_LIMIT_BYTES_DEFAULT),
        timeout_ms: manifest.timeout_ms,
        max_response_bytes: manifest
            .wasm
            .max_response_bytes
            .unwrap_or(WASM_SANDBOX_MAX_RESPONSE_BYTES_DEFAULT),
    }
}

fn wasm_runtime_capabilities_from_manifest(
    manifest: &ExtensionManifest,
) -> WasmSandboxCapabilityProfile {
    let filesystem_mode = match manifest.wasm.filesystem_mode {
        Some(ExtensionWasmFilesystemMode::Deny) | None => WasmSandboxFilesystemMode::Deny,
        Some(ExtensionWasmFilesystemMode::ReadOnly) => WasmSandboxFilesystemMode::ReadOnly,
        Some(ExtensionWasmFilesystemMode::ReadWrite) => WasmSandboxFilesystemMode::ReadWrite,
    };
    let network_mode = match manifest.wasm.network_mode {
        Some(ExtensionWasmNetworkMode::Deny) | None => WasmSandboxNetworkMode::Deny,
        Some(ExtensionWasmNetworkMode::Allow) => WasmSandboxNetworkMode::Allow,
    };
    WasmSandboxCapabilityProfile {
        filesystem_mode,
        network_mode,
        env_allowlist: manifest.wasm.env_allowlist.clone(),
    }
}

fn execute_extension_runtime_with_request(
    runtime: &ExtensionRuntime,
    entrypoint: &Path,
    request_json: &str,
    timeout_ms: u64,
    wasm_limits: Option<WasmSandboxLimits>,
    wasm_capabilities: Option<WasmSandboxCapabilityProfile>,
) -> Result<ExtensionHookExecutionOutput> {
    let started_at = Instant::now();
    match runtime {
        ExtensionRuntime::Process => {
            let output = run_extension_process_with_timeout(entrypoint, request_json, timeout_ms)?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stderr = stderr.trim();
                if stderr.is_empty() {
                    bail!(
                        "extension process exited with non-zero status {}",
                        output.status
                    );
                }
                bail!(
                    "extension process exited with non-zero status {}: {}",
                    output.status,
                    stderr
                );
            }
            let response_raw = String::from_utf8(output.stdout)
                .context("extension process output is not valid UTF-8")?;
            if response_raw.trim().is_empty() {
                bail!("extension process returned empty response");
            }
            let response_json = serde_json::from_str::<Value>(&response_raw)
                .context("extension process response must be valid JSON")?;
            if !response_json.is_object() {
                bail!("extension process response must be a JSON object");
            }
            let response = serde_json::to_string(&response_json)
                .context("failed to serialize extension process response JSON")?;
            Ok(ExtensionHookExecutionOutput {
                response_bytes: response.len(),
                response,
                duration_ms: started_at.elapsed().as_millis() as u64,
                reason_codes: vec![EXTENSION_PROCESS_EXECUTION_SUCCEEDED_REASON_CODE.to_string()],
                diagnostics: vec![],
            })
        }
        ExtensionRuntime::Wasm => {
            let limits = wasm_limits.unwrap_or(WasmSandboxLimits {
                timeout_ms,
                ..WasmSandboxLimits::default()
            });
            let capabilities = wasm_capabilities.unwrap_or_default();
            let report = execute_wasm_sandbox_sync(WasmSandboxExecutionRequest {
                module_path: entrypoint.to_path_buf(),
                request_json: request_json.to_string(),
                limits,
                capabilities,
            })
            .map_err(|error| {
                anyhow!(
                    "extension wasm runtime failed: reason_code={} message={} diagnostics={}",
                    error.reason_code,
                    error.message,
                    if error.diagnostics.is_empty() {
                        "none".to_string()
                    } else {
                        error.diagnostics.join("; ")
                    }
                )
            })?;
            let response_json = serde_json::from_str::<Value>(&report.response_json)
                .context("extension wasm response must be valid JSON")?;
            if !response_json.is_object() {
                bail!("extension wasm response must be a JSON object");
            }
            let response = serde_json::to_string(&response_json)
                .context("failed to serialize extension wasm response JSON")?;
            Ok(ExtensionHookExecutionOutput {
                response_bytes: response.len(),
                response,
                duration_ms: started_at.elapsed().as_millis() as u64,
                reason_codes: report.reason_codes,
                diagnostics: report.diagnostics,
            })
        }
    }
}

pub fn validate_extension_manifest(path: &Path) -> Result<ExtensionManifestSummary> {
    let (_, summary) = load_and_validate_extension_manifest(path)?;
    Ok(summary)
}

pub fn load_and_validate_extension_manifest(
    path: &Path,
) -> Result<(ExtensionManifest, ExtensionManifestSummary)> {
    let manifest = load_extension_manifest(path)?;
    validate_manifest_schema(&manifest)?;
    validate_manifest_identifiers(&manifest)?;
    validate_entrypoint_path(&manifest.entrypoint)?;
    validate_unique(&manifest.hooks, "hooks")?;
    validate_unique(&manifest.permissions, "permissions")?;
    validate_tool_registrations(&manifest.tools)?;
    validate_command_registrations(&manifest.commands)?;
    validate_timeout_ms(manifest.timeout_ms)?;
    validate_wasm_runtime_config(&manifest)?;
    let summary = ExtensionManifestSummary {
        manifest_path: path.to_path_buf(),
        id: manifest.id.clone(),
        version: manifest.version.clone(),
        runtime: manifest.runtime.as_str().to_string(),
        entrypoint: manifest.entrypoint.clone(),
        hook_count: manifest.hooks.len(),
        permission_count: manifest.permissions.len(),
        timeout_ms: manifest.timeout_ms,
    };
    Ok((manifest, summary))
}

pub fn render_extension_manifest_report(
    summary: &ExtensionManifestSummary,
    manifest: &ExtensionManifest,
) -> String {
    let mut hooks = manifest
        .hooks
        .iter()
        .map(|hook| hook.as_str().to_string())
        .collect::<Vec<_>>();
    hooks.sort();

    let mut permissions = manifest
        .permissions
        .iter()
        .map(|permission| permission.as_str().to_string())
        .collect::<Vec<_>>();
    permissions.sort();

    let hook_lines = if hooks.is_empty() {
        "- none".to_string()
    } else {
        hooks
            .iter()
            .map(|hook| format!("- {hook}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let permission_lines = if permissions.is_empty() {
        "- none".to_string()
    } else {
        permissions
            .iter()
            .map(|permission| format!("- {permission}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let mut tools = manifest
        .tools
        .iter()
        .map(|tool| tool.name.trim().to_string())
        .collect::<Vec<_>>();
    tools.sort();
    let tool_lines = if tools.is_empty() {
        "- none".to_string()
    } else {
        tools
            .iter()
            .map(|tool| format!("- {tool}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let mut commands = manifest
        .commands
        .iter()
        .filter_map(|command| normalize_extension_command_name(&command.name).ok())
        .collect::<Vec<_>>();
    commands.sort();
    let command_lines = if commands.is_empty() {
        "- none".to_string()
    } else {
        commands
            .iter()
            .map(|command| format!("- {command}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "extension show:\n- path: {}\n- id: {}\n- version: {}\n- runtime: {}\n- entrypoint: {}\n- timeout_ms: {}\n- hooks ({}):\n{}\n- permissions ({}):\n{}\n- tools ({}):\n{}\n- commands ({}):\n{}",
        summary.manifest_path.display(),
        summary.id,
        summary.version,
        summary.runtime,
        summary.entrypoint,
        summary.timeout_ms,
        summary.hook_count,
        hook_lines,
        summary.permission_count,
        permission_lines,
        tools.len(),
        tool_lines,
        commands.len(),
        command_lines
    )
}

pub fn list_extension_manifests(root: &Path) -> Result<ExtensionListReport> {
    if !root.exists() {
        return Ok(ExtensionListReport {
            list_root: root.to_path_buf(),
            entries: vec![],
            invalid_entries: vec![],
        });
    }
    if !root.is_dir() {
        bail!(
            "extension list root '{}' is not a directory",
            root.display()
        );
    }

    let mut entries = Vec::new();
    let mut invalid_entries = Vec::new();
    for manifest_path in discover_manifest_paths(root)? {
        match validate_extension_manifest(&manifest_path) {
            Ok(summary) => entries.push(ExtensionListEntry {
                manifest_path: summary.manifest_path,
                id: summary.id,
                version: summary.version,
                runtime: summary.runtime,
            }),
            Err(error) => invalid_entries.push(ExtensionListInvalidEntry {
                manifest_path,
                error: error.to_string(),
            }),
        }
    }
    entries.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.version.cmp(&right.version))
            .then_with(|| left.manifest_path.cmp(&right.manifest_path))
    });
    invalid_entries.sort_by(|left, right| left.manifest_path.cmp(&right.manifest_path));

    Ok(ExtensionListReport {
        list_root: root.to_path_buf(),
        entries,
        invalid_entries,
    })
}

fn discover_manifest_paths(root: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    let read_dir = fs::read_dir(root)
        .with_context(|| format!("failed to read extension root {}", root.display()))?;
    for entry in read_dir {
        let entry = entry
            .with_context(|| format!("failed to inspect extension root {}", root.display()))?;
        let path = entry.path();
        if path.is_dir() {
            let manifest_path = path.join("extension.json");
            if manifest_path.is_file() {
                paths.push(manifest_path);
            }
            continue;
        }
        if path.extension().and_then(|extension| extension.to_str()) == Some("json")
            && path.is_file()
        {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

pub fn render_extension_list_report(report: &ExtensionListReport) -> String {
    let mut lines = vec![format!(
        "extension list: root={} count={} invalid={}",
        report.list_root.display(),
        report.entries.len(),
        report.invalid_entries.len()
    )];
    for entry in &report.entries {
        lines.push(format!(
            "extension: id={} version={} runtime={} manifest={}",
            entry.id,
            entry.version,
            entry.runtime,
            entry.manifest_path.display()
        ));
    }
    for invalid in &report.invalid_entries {
        lines.push(format!(
            "invalid: manifest={} error={}",
            invalid.manifest_path.display(),
            invalid.error
        ));
    }
    lines.join("\n")
}

pub fn dispatch_extension_runtime_hook(
    root: &Path,
    hook_raw: &str,
    payload: &Value,
) -> ExtensionRuntimeHookDispatchSummary {
    let mut summary = ExtensionRuntimeHookDispatchSummary {
        root: root.to_path_buf(),
        hook: hook_raw.trim().to_string(),
        discovered: 0,
        eligible: 0,
        executed: 0,
        failed: 0,
        skipped_invalid: 0,
        skipped_unsupported_runtime: 0,
        skipped_undeclared_hook: 0,
        skipped_permission_denied: 0,
        executed_ids: Vec::new(),
        diagnostics: Vec::new(),
    };

    let hook = match ExtensionHook::parse(hook_raw) {
        Ok(hook) => hook,
        Err(error) => {
            summary
                .diagnostics
                .push(format!("extension runtime: unsupported hook: {error}"));
            return summary;
        }
    };

    let (loaded, invalid_diagnostics) = match discover_loaded_extension_manifests(root) {
        Ok(loaded) => loaded,
        Err(error) => {
            summary
                .diagnostics
                .push(format!("extension runtime: {error}"));
            return summary;
        }
    };

    summary.skipped_invalid = invalid_diagnostics.len();
    summary.diagnostics.extend(invalid_diagnostics);
    summary.discovered = loaded.len();
    for loaded_manifest in loaded {
        if !is_supported_extension_runtime(&loaded_manifest.manifest.runtime) {
            summary.skipped_unsupported_runtime += 1;
            summary.diagnostics.push(format!(
                "extension runtime: skipped id={} manifest={} reason=unsupported runtime={}",
                loaded_manifest.summary.id,
                loaded_manifest.summary.manifest_path.display(),
                loaded_manifest.manifest.runtime.as_str()
            ));
            continue;
        }
        if !loaded_manifest.manifest.hooks.contains(&hook) {
            summary.skipped_undeclared_hook += 1;
            continue;
        }
        if let Some(required_permission) = required_permission_for_hook(&hook) {
            if !loaded_manifest
                .manifest
                .permissions
                .contains(&required_permission)
            {
                summary.skipped_permission_denied += 1;
                summary.diagnostics.push(format!(
                    "extension runtime: hook={} id={} manifest={} denied: missing required permission={}",
                    hook.as_str(),
                    loaded_manifest.summary.id,
                    loaded_manifest.summary.manifest_path.display(),
                    required_permission.as_str()
                ));
                continue;
            }
        }

        summary.eligible += 1;
        match execute_extension_process_hook_with_loaded(
            &loaded_manifest.manifest,
            &loaded_manifest.summary,
            &hook,
            payload,
        ) {
            Ok(exec_summary) => {
                summary.executed += 1;
                summary
                    .executed_ids
                    .push(format!("{}@{}", exec_summary.id, exec_summary.version));
            }
            Err(error) => {
                summary.failed += 1;
                summary.diagnostics.push(format!(
                    "extension runtime: hook={} id={} manifest={} failed: {}",
                    hook.as_str(),
                    loaded_manifest.summary.id,
                    loaded_manifest.summary.manifest_path.display(),
                    error
                ));
            }
        }
    }

    summary
}

pub fn apply_extension_message_transforms(
    root: &Path,
    prompt: &str,
) -> ExtensionMessageTransformResult {
    let mut result = ExtensionMessageTransformResult {
        root: root.to_path_buf(),
        prompt: prompt.to_string(),
        executed: 0,
        applied: 0,
        failed: 0,
        skipped_invalid: 0,
        skipped_unsupported_runtime: 0,
        skipped_undeclared_hook: 0,
        skipped_permission_denied: 0,
        applied_ids: Vec::new(),
        diagnostics: Vec::new(),
    };
    let hook = ExtensionHook::MessageTransform;

    let (loaded, invalid_diagnostics) = match discover_loaded_extension_manifests(root) {
        Ok(loaded) => loaded,
        Err(error) => {
            result
                .diagnostics
                .push(format!("extension runtime: {error}"));
            return result;
        }
    };

    result.skipped_invalid = invalid_diagnostics.len();
    result.diagnostics.extend(invalid_diagnostics);

    for loaded_manifest in loaded {
        if !is_supported_extension_runtime(&loaded_manifest.manifest.runtime) {
            result.skipped_unsupported_runtime += 1;
            continue;
        }
        if !loaded_manifest.manifest.hooks.contains(&hook) {
            result.skipped_undeclared_hook += 1;
            continue;
        }
        if let Some(required_permission) = required_permission_for_hook(&hook) {
            if !loaded_manifest
                .manifest
                .permissions
                .contains(&required_permission)
            {
                result.skipped_permission_denied += 1;
                result.diagnostics.push(format!(
                    "extension runtime: hook={} id={} manifest={} denied: missing required permission={}",
                    hook.as_str(),
                    loaded_manifest.summary.id,
                    loaded_manifest.summary.manifest_path.display(),
                    required_permission.as_str()
                ));
                continue;
            }
        }

        result.executed += 1;
        let payload = serde_json::json!({
            "schema_version": EXTENSION_HOOK_PAYLOAD_SCHEMA_VERSION,
            "hook": hook.as_str(),
            "emitted_at_ms": current_unix_timestamp_ms(),
            "prompt": result.prompt.clone(),
            "data": {
                "prompt": result.prompt.clone(),
            },
        });
        match execute_extension_process_hook_with_loaded(
            &loaded_manifest.manifest,
            &loaded_manifest.summary,
            &hook,
            &payload,
        ) {
            Ok(exec_summary) => {
                match parse_message_transform_response_prompt(&exec_summary.response) {
                    Ok(Some(next_prompt)) => {
                        result.prompt = next_prompt;
                        result.applied += 1;
                        result
                            .applied_ids
                            .push(format!("{}@{}", exec_summary.id, exec_summary.version));
                    }
                    Ok(None) => {}
                    Err(error) => {
                        result.diagnostics.push(format!(
                            "extension runtime: hook={} id={} manifest={} invalid response: {}",
                            hook.as_str(),
                            loaded_manifest.summary.id,
                            loaded_manifest.summary.manifest_path.display(),
                            error
                        ));
                    }
                }
            }
            Err(error) => {
                result.failed += 1;
                result.diagnostics.push(format!(
                    "extension runtime: hook={} id={} manifest={} failed: {}",
                    hook.as_str(),
                    loaded_manifest.summary.id,
                    loaded_manifest.summary.manifest_path.display(),
                    error
                ));
            }
        }
    }

    result
}

pub fn evaluate_extension_policy_override(
    root: &Path,
    payload: &Value,
) -> ExtensionPolicyOverrideResult {
    let mut result = ExtensionPolicyOverrideResult {
        root: root.to_path_buf(),
        allowed: true,
        denied_by: None,
        reason: None,
        evaluated: 0,
        denied: 0,
        permission_denied: 0,
        skipped_invalid: 0,
        skipped_unsupported_runtime: 0,
        skipped_undeclared_hook: 0,
        diagnostics: Vec::new(),
    };
    let hook = ExtensionHook::PolicyOverride;

    let (loaded, invalid_diagnostics) = match discover_loaded_extension_manifests(root) {
        Ok(loaded) => loaded,
        Err(error) => {
            result.allowed = false;
            result.reason = Some(format!("failed to discover extension manifests: {error}"));
            result
                .diagnostics
                .push(format!("extension runtime: {error}"));
            return result;
        }
    };

    result.skipped_invalid = invalid_diagnostics.len();
    result.diagnostics.extend(invalid_diagnostics);

    for loaded_manifest in loaded {
        if !is_supported_extension_runtime(&loaded_manifest.manifest.runtime) {
            result.skipped_unsupported_runtime += 1;
            continue;
        }
        if !loaded_manifest.manifest.hooks.contains(&hook) {
            result.skipped_undeclared_hook += 1;
            continue;
        }

        result.evaluated += 1;
        if let Some(required_permission) = required_permission_for_hook(&hook) {
            if !loaded_manifest
                .manifest
                .permissions
                .contains(&required_permission)
            {
                result.allowed = false;
                result.denied += 1;
                result.permission_denied += 1;
                result.denied_by = Some(format!(
                    "{}@{}",
                    loaded_manifest.summary.id, loaded_manifest.summary.version
                ));
                result.reason = Some(format!(
                    "policy override hook requires '{}' permission",
                    required_permission.as_str()
                ));
                result.diagnostics.push(format!(
                    "extension runtime: hook={} id={} manifest={} denied: missing required permission={}",
                    hook.as_str(),
                    loaded_manifest.summary.id,
                    loaded_manifest.summary.manifest_path.display(),
                    required_permission.as_str()
                ));
                break;
            }
        }
        let exec_summary = match execute_extension_process_hook_with_loaded(
            &loaded_manifest.manifest,
            &loaded_manifest.summary,
            &hook,
            payload,
        ) {
            Ok(summary) => summary,
            Err(error) => {
                result.allowed = false;
                result.denied += 1;
                result.denied_by = Some(format!(
                    "{}@{}",
                    loaded_manifest.summary.id, loaded_manifest.summary.version
                ));
                result.reason = Some(format!("policy override hook execution failed: {}", error));
                result.diagnostics.push(format!(
                    "extension runtime: hook={} id={} manifest={} failed: {}",
                    hook.as_str(),
                    loaded_manifest.summary.id,
                    loaded_manifest.summary.manifest_path.display(),
                    error
                ));
                break;
            }
        };
        let parsed = match parse_policy_override_response(&exec_summary.response) {
            Ok(parsed) => parsed,
            Err(error) => {
                result.allowed = false;
                result.denied += 1;
                result.denied_by = Some(format!("{}@{}", exec_summary.id, exec_summary.version));
                result.reason = Some(format!(
                    "policy override hook returned invalid response: {}",
                    error
                ));
                result.diagnostics.push(format!(
                    "extension runtime: hook={} id={} manifest={} invalid response: {}",
                    hook.as_str(),
                    loaded_manifest.summary.id,
                    loaded_manifest.summary.manifest_path.display(),
                    error
                ));
                break;
            }
        };

        if parsed.decision == PolicyOverrideDecision::Deny {
            result.allowed = false;
            result.denied += 1;
            result.denied_by = Some(format!("{}@{}", exec_summary.id, exec_summary.version));
            result.reason = Some(
                parsed
                    .reason
                    .unwrap_or_else(|| "extension policy override denied command".to_string()),
            );
            break;
        }
    }

    result
}

pub fn discover_extension_runtime_registrations(
    root: &Path,
    reserved_tool_names: &[&str],
    builtin_command_names: &[&str],
) -> ExtensionRuntimeRegistrationSummary {
    let mut summary = ExtensionRuntimeRegistrationSummary {
        root: root.to_path_buf(),
        discovered: 0,
        registered_tools: Vec::new(),
        registered_commands: Vec::new(),
        skipped_invalid: 0,
        skipped_unsupported_runtime: 0,
        skipped_permission_denied: 0,
        skipped_name_conflict: 0,
        diagnostics: Vec::new(),
    };

    let (loaded, invalid_diagnostics) = match discover_loaded_extension_manifests(root) {
        Ok(loaded) => loaded,
        Err(error) => {
            summary
                .diagnostics
                .push(format!("extension runtime: {error}"));
            return summary;
        }
    };
    summary.skipped_invalid = invalid_diagnostics.len();
    summary.diagnostics.extend(invalid_diagnostics);
    summary.discovered = loaded.len();

    let mut seen_tools = HashSet::new();
    let mut seen_commands = HashSet::new();

    for loaded_manifest in loaded {
        if !is_supported_extension_runtime(&loaded_manifest.manifest.runtime) {
            summary.skipped_unsupported_runtime += 1;
            summary.diagnostics.push(format!(
                "extension runtime: skipped id={} manifest={} reason=unsupported runtime={}",
                loaded_manifest.summary.id,
                loaded_manifest.summary.manifest_path.display(),
                loaded_manifest.manifest.runtime.as_str()
            ));
            continue;
        }

        let entrypoint = match resolve_extension_entrypoint(
            &loaded_manifest.summary.manifest_path,
            &loaded_manifest.manifest.entrypoint,
        ) {
            Ok(entrypoint) => entrypoint,
            Err(error) => {
                summary.skipped_invalid += 1;
                summary.diagnostics.push(format!(
                    "extension runtime: skipped id={} manifest={} reason=invalid entrypoint: {}",
                    loaded_manifest.summary.id,
                    loaded_manifest.summary.manifest_path.display(),
                    error
                ));
                continue;
            }
        };

        let has_run_commands = loaded_manifest
            .manifest
            .permissions
            .contains(&ExtensionPermission::RunCommands);
        let runtime = loaded_manifest.manifest.runtime.as_str().to_string();
        let wasm_limits = match loaded_manifest.manifest.runtime {
            ExtensionRuntime::Wasm => {
                Some(wasm_runtime_limits_from_manifest(&loaded_manifest.manifest))
            }
            ExtensionRuntime::Process => None,
        };
        let wasm_capabilities = match loaded_manifest.manifest.runtime {
            ExtensionRuntime::Wasm => Some(wasm_runtime_capabilities_from_manifest(
                &loaded_manifest.manifest,
            )),
            ExtensionRuntime::Process => None,
        };

        for tool in &loaded_manifest.manifest.tools {
            let tool_name = tool.name.trim().to_string();
            if !has_run_commands {
                summary.skipped_permission_denied += 1;
                summary.diagnostics.push(format!(
                    "extension runtime: tool={} id={} manifest={} denied: missing required permission={}",
                    tool_name,
                    loaded_manifest.summary.id,
                    loaded_manifest.summary.manifest_path.display(),
                    ExtensionPermission::RunCommands.as_str()
                ));
                continue;
            }
            if reserved_tool_names.contains(&tool_name.as_str()) {
                summary.skipped_name_conflict += 1;
                summary.diagnostics.push(format!(
                    "extension runtime: tool={} id={} manifest={} denied: name conflicts with reserved built-in tool '{}'",
                    tool_name,
                    loaded_manifest.summary.id,
                    loaded_manifest.summary.manifest_path.display(),
                    tool_name
                ));
                continue;
            }
            if !seen_tools.insert(tool_name.clone()) {
                summary.skipped_name_conflict += 1;
                summary.diagnostics.push(format!(
                    "extension runtime: tool={} id={} manifest={} denied: duplicate extension tool name",
                    tool_name,
                    loaded_manifest.summary.id,
                    loaded_manifest.summary.manifest_path.display()
                ));
                continue;
            }
            summary.registered_tools.push(ExtensionRegisteredTool {
                name: tool_name,
                description: tool.description.trim().to_string(),
                parameters: tool.parameters.clone(),
                runtime: runtime.clone(),
                extension_id: loaded_manifest.summary.id.clone(),
                extension_version: loaded_manifest.summary.version.clone(),
                manifest_path: loaded_manifest.summary.manifest_path.clone(),
                entrypoint: entrypoint.clone(),
                timeout_ms: loaded_manifest.manifest.timeout_ms,
                wasm_limits: wasm_limits.clone(),
                wasm_capabilities: wasm_capabilities.clone(),
            });
        }

        for command in &loaded_manifest.manifest.commands {
            let command_name = match normalize_extension_command_name(&command.name) {
                Ok(name) => name,
                Err(error) => {
                    summary.skipped_invalid += 1;
                    summary.diagnostics.push(format!(
                        "extension runtime: command={} id={} manifest={} denied: invalid name: {}",
                        command.name.trim(),
                        loaded_manifest.summary.id,
                        loaded_manifest.summary.manifest_path.display(),
                        error
                    ));
                    continue;
                }
            };
            if !has_run_commands {
                summary.skipped_permission_denied += 1;
                summary.diagnostics.push(format!(
                    "extension runtime: command={} id={} manifest={} denied: missing required permission={}",
                    command_name,
                    loaded_manifest.summary.id,
                    loaded_manifest.summary.manifest_path.display(),
                    ExtensionPermission::RunCommands.as_str()
                ));
                continue;
            }
            if builtin_command_names.contains(&command_name.as_str()) {
                summary.skipped_name_conflict += 1;
                summary.diagnostics.push(format!(
                    "extension runtime: command={} id={} manifest={} denied: name conflicts with built-in command",
                    command_name,
                    loaded_manifest.summary.id,
                    loaded_manifest.summary.manifest_path.display()
                ));
                continue;
            }
            if !seen_commands.insert(command_name.clone()) {
                summary.skipped_name_conflict += 1;
                summary.diagnostics.push(format!(
                    "extension runtime: command={} id={} manifest={} denied: duplicate extension command name",
                    command_name,
                    loaded_manifest.summary.id,
                    loaded_manifest.summary.manifest_path.display()
                ));
                continue;
            }
            summary
                .registered_commands
                .push(ExtensionRegisteredCommand {
                    name: command_name,
                    description: command.description.trim().to_string(),
                    usage: command
                        .usage
                        .as_ref()
                        .map(|usage| usage.trim().to_string())
                        .filter(|usage| !usage.is_empty()),
                    runtime: runtime.clone(),
                    extension_id: loaded_manifest.summary.id.clone(),
                    extension_version: loaded_manifest.summary.version.clone(),
                    manifest_path: loaded_manifest.summary.manifest_path.clone(),
                    entrypoint: entrypoint.clone(),
                    timeout_ms: loaded_manifest.manifest.timeout_ms,
                    wasm_limits: wasm_limits.clone(),
                    wasm_capabilities: wasm_capabilities.clone(),
                });
        }
    }

    summary.registered_tools.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.extension_id.cmp(&right.extension_id))
            .then_with(|| left.extension_version.cmp(&right.extension_version))
            .then_with(|| left.manifest_path.cmp(&right.manifest_path))
    });
    summary.registered_commands.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.extension_id.cmp(&right.extension_id))
            .then_with(|| left.extension_version.cmp(&right.extension_version))
            .then_with(|| left.manifest_path.cmp(&right.manifest_path))
    });

    summary
}

pub fn dispatch_extension_registered_command(
    registered_commands: &[ExtensionRegisteredCommand],
    command_name: &str,
    command_args: &str,
) -> Result<Option<ExtensionRegisteredCommandResult>> {
    let Some(command) = registered_commands
        .iter()
        .find(|candidate| candidate.name == command_name)
    else {
        return Ok(None);
    };

    let payload = serde_json::json!({
        "schema_version": EXTENSION_HOOK_PAYLOAD_SCHEMA_VERSION,
        "kind": "command-call",
        "command": {
            "name": command.name,
            "args": command_args,
        },
    });
    let request = serde_json::json!({
        "hook": "command-call",
        "payload": payload,
        "manifest_id": command.extension_id,
        "manifest_version": command.extension_version,
    });
    let request_json = serde_json::to_string(&request)
        .context("failed to serialize extension command request payload")?;
    let runtime = parse_registered_extension_runtime(&command.runtime)?;
    let execution = execute_extension_runtime_with_request(
        &runtime,
        &command.entrypoint,
        &request_json,
        command.timeout_ms,
        command.wasm_limits.clone(),
        command.wasm_capabilities.clone(),
    )?;
    parse_extension_registered_command_response(&command.name, &execution.response).map(Some)
}

pub fn execute_extension_registered_tool(
    tool: &ExtensionRegisteredTool,
    arguments: &Value,
) -> Result<ExtensionRegisteredToolResult> {
    let payload = serde_json::json!({
        "schema_version": EXTENSION_HOOK_PAYLOAD_SCHEMA_VERSION,
        "kind": "tool-call",
        "tool": {
            "name": tool.name,
            "arguments": arguments,
        },
    });
    let request = serde_json::json!({
        "hook": "tool-call",
        "payload": payload,
        "manifest_id": tool.extension_id,
        "manifest_version": tool.extension_version,
    });
    let request_json = serde_json::to_string(&request)
        .context("failed to serialize extension tool request payload")?;
    let runtime = parse_registered_extension_runtime(&tool.runtime)?;
    let execution = execute_extension_runtime_with_request(
        &runtime,
        &tool.entrypoint,
        &request_json,
        tool.timeout_ms,
        tool.wasm_limits.clone(),
        tool.wasm_capabilities.clone(),
    )?;
    parse_extension_registered_tool_response(&tool.name, &execution.response)
}

fn parse_extension_registered_command_response(
    command_name: &str,
    response_json: &str,
) -> Result<ExtensionRegisteredCommandResult> {
    let value = serde_json::from_str::<Value>(response_json).with_context(|| {
        format!(
            "extension command '{}' response must be valid JSON object",
            command_name
        )
    })?;
    let object = value.as_object().ok_or_else(|| {
        anyhow!(
            "extension command '{}' response must be a JSON object",
            command_name
        )
    })?;
    let output = object
        .get("output")
        .or_else(|| object.get("message"))
        .map(|value| {
            value
                .as_str()
                .map(|output| output.trim().to_string())
                .ok_or_else(|| {
                    anyhow!(
                        "extension command '{}' response field 'output' must be a string",
                        command_name
                    )
                })
        })
        .transpose()?
        .filter(|output| !output.is_empty());
    let action = object
        .get("action")
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| {
                    anyhow!(
                        "extension command '{}' response field 'action' must be a string",
                        command_name
                    )
                })
                .and_then(|action| match action {
                    EXTENSION_COMMAND_RESPONSE_ACTION_CONTINUE => {
                        Ok(ExtensionRegisteredCommandAction::Continue)
                    }
                    EXTENSION_COMMAND_RESPONSE_ACTION_EXIT => {
                        Ok(ExtensionRegisteredCommandAction::Exit)
                    }
                    other => bail!(
                        "extension command '{}' response field 'action' must be '{}' or '{}', got '{}'",
                        command_name,
                        EXTENSION_COMMAND_RESPONSE_ACTION_CONTINUE,
                        EXTENSION_COMMAND_RESPONSE_ACTION_EXIT,
                        other
                    ),
                })
        })
        .transpose()?
        .unwrap_or(ExtensionRegisteredCommandAction::Continue);

    Ok(ExtensionRegisteredCommandResult { output, action })
}

fn parse_extension_registered_tool_response(
    tool_name: &str,
    response_json: &str,
) -> Result<ExtensionRegisteredToolResult> {
    let value = serde_json::from_str::<Value>(response_json).with_context(|| {
        format!(
            "extension tool '{}' response must be valid JSON object",
            tool_name
        )
    })?;
    let object = value.as_object().ok_or_else(|| {
        anyhow!(
            "extension tool '{}' response must be a JSON object",
            tool_name
        )
    })?;
    let content = object.get("content").cloned().ok_or_else(|| {
        anyhow!(
            "extension tool '{}' response must include field 'content'",
            tool_name
        )
    })?;
    let is_error = object
        .get("is_error")
        .map(|value| {
            value.as_bool().ok_or_else(|| {
                anyhow!(
                    "extension tool '{}' field 'is_error' must be a boolean",
                    tool_name
                )
            })
        })
        .transpose()?
        .unwrap_or(false);

    Ok(ExtensionRegisteredToolResult { content, is_error })
}

fn discover_loaded_extension_manifests(
    root: &Path,
) -> Result<(Vec<LoadedExtensionManifest>, Vec<String>)> {
    if !root.exists() {
        return Ok((Vec::new(), Vec::new()));
    }
    if !root.is_dir() {
        bail!(
            "extension runtime root '{}' is not a directory",
            root.display()
        );
    }

    let mut loaded = Vec::new();
    let mut invalid_diagnostics = Vec::new();
    for manifest_path in discover_manifest_paths(root)? {
        match load_and_validate_extension_manifest(&manifest_path) {
            Ok((manifest, summary)) => loaded.push(LoadedExtensionManifest { manifest, summary }),
            Err(error) => invalid_diagnostics.push(format!(
                "extension runtime: skipped invalid manifest={} error={error}",
                manifest_path.display()
            )),
        }
    }

    loaded.sort_by(|left, right| {
        left.summary
            .id
            .cmp(&right.summary.id)
            .then_with(|| left.summary.version.cmp(&right.summary.version))
            .then_with(|| left.summary.manifest_path.cmp(&right.summary.manifest_path))
    });

    Ok((loaded, invalid_diagnostics))
}

fn parse_message_transform_response_prompt(response_json: &str) -> Result<Option<String>> {
    let value = serde_json::from_str::<Value>(response_json)
        .context("message-transform response must be valid JSON object")?;
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("message-transform response must be a JSON object"))?;
    let Some(prompt_value) = object.get("prompt") else {
        return Ok(None);
    };
    let prompt = prompt_value
        .as_str()
        .ok_or_else(|| anyhow!("message-transform response field 'prompt' must be a string"))?;
    if prompt.trim().is_empty() {
        bail!("message-transform response field 'prompt' must not be empty");
    }
    Ok(Some(prompt.to_string()))
}

fn parse_policy_override_response(response_json: &str) -> Result<PolicyOverrideResponse> {
    let value = serde_json::from_str::<Value>(response_json)
        .context("policy-override response must be valid JSON object")?;
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("policy-override response must be a JSON object"))?;
    let decision_raw = object
        .get("decision")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("policy-override response must include string field 'decision'"))?;
    let decision = match decision_raw {
        "allow" => PolicyOverrideDecision::Allow,
        "deny" => PolicyOverrideDecision::Deny,
        other => bail!(
            "policy-override response field 'decision' must be 'allow' or 'deny', got '{}'",
            other
        ),
    };
    let reason = object.get("reason").map(|value| {
        value
            .as_str()
            .map(|reason| reason.trim().to_string())
            .ok_or_else(|| anyhow!("policy-override response field 'reason' must be a string"))
    });
    let reason = match reason {
        Some(Ok(reason)) if reason.is_empty() => None,
        Some(Ok(reason)) => Some(reason),
        Some(Err(error)) => return Err(error),
        None => None,
    };
    Ok(PolicyOverrideResponse { decision, reason })
}

pub fn load_extension_exec_payload(path: &Path) -> Result<Value> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read extension payload {}", path.display()))?;
    let payload = serde_json::from_str::<Value>(&raw)
        .with_context(|| format!("failed to parse extension payload {}", path.display()))?;
    if !payload.is_object() {
        bail!("extension payload must be a JSON object");
    }
    Ok(payload)
}

pub fn execute_extension_process_hook(
    manifest_path: &Path,
    hook_raw: &str,
    payload: &Value,
) -> Result<ExtensionExecSummary> {
    let (manifest, summary) = load_and_validate_extension_manifest(manifest_path)?;
    let hook = ExtensionHook::parse(hook_raw)?;
    execute_extension_process_hook_with_loaded(&manifest, &summary, &hook, payload)
}

fn execute_extension_process_hook_with_loaded(
    manifest: &ExtensionManifest,
    summary: &ExtensionManifestSummary,
    hook: &ExtensionHook,
    payload: &Value,
) -> Result<ExtensionExecSummary> {
    if !is_supported_extension_runtime(&manifest.runtime) {
        bail!(
            "extension manifest runtime '{}' is not supported for extension exec",
            manifest.runtime.as_str()
        );
    }
    if !manifest.hooks.contains(hook) {
        bail!(
            "extension manifest '{}' does not declare hook '{}'",
            summary.id,
            hook.as_str()
        );
    }
    if let Some(required_permission) = required_permission_for_hook(hook) {
        if !manifest.permissions.contains(&required_permission) {
            bail!(
                "extension manifest '{}' hook '{}' requires permission '{}'",
                summary.id,
                hook.as_str(),
                required_permission.as_str()
            );
        }
    }
    let payload_object = payload
        .as_object()
        .ok_or_else(|| anyhow!("extension payload must be a JSON object"))?;
    let request = serde_json::json!({
        "hook": hook.as_str(),
        "payload": payload_object,
        "manifest_id": manifest.id,
        "manifest_version": manifest.version,
    });
    let request_json = serde_json::to_string(&request)
        .context("failed to serialize extension execution request payload")?;

    let entrypoint = resolve_extension_entrypoint(&summary.manifest_path, &manifest.entrypoint)?;
    let execution = execute_extension_runtime_with_request(
        &manifest.runtime,
        &entrypoint,
        &request_json,
        manifest.timeout_ms,
        Some(wasm_runtime_limits_from_manifest(manifest)),
        Some(wasm_runtime_capabilities_from_manifest(manifest)),
    )?;

    Ok(ExtensionExecSummary {
        manifest_path: summary.manifest_path.clone(),
        id: summary.id.clone(),
        version: summary.version.clone(),
        runtime: summary.runtime.clone(),
        hook: hook.as_str().to_string(),
        timeout_ms: manifest.timeout_ms,
        duration_ms: execution.duration_ms,
        response_bytes: execution.response_bytes,
        response: execution.response,
        reason_codes: execution.reason_codes,
        diagnostics: execution.diagnostics,
    })
}

fn resolve_extension_entrypoint(manifest_path: &Path, entrypoint: &str) -> Result<PathBuf> {
    let manifest_dir = manifest_path.parent().ok_or_else(|| {
        anyhow!(
            "extension manifest path '{}' has no parent directory",
            manifest_path.display()
        )
    })?;
    let manifest_dir = manifest_dir.canonicalize().with_context(|| {
        format!(
            "failed to resolve manifest directory {}",
            manifest_dir.display()
        )
    })?;
    let candidate = manifest_dir.join(entrypoint);
    let resolved = candidate.canonicalize().with_context(|| {
        format!(
            "failed to resolve extension entrypoint {}",
            candidate.display()
        )
    })?;
    if !resolved.starts_with(&manifest_dir) {
        bail!(
            "extension entrypoint '{}' resolves outside manifest directory",
            entrypoint
        );
    }
    if !resolved.is_file() {
        bail!(
            "extension entrypoint '{}' is not a regular file",
            resolved.display()
        );
    }
    Ok(resolved)
}

#[cfg(test)]
fn extension_process_test_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .expect("extension process test lock")
}

fn format_extension_process_stdin_payload(request_json: &str) -> String {
    let mut payload = String::with_capacity(request_json.len() + 1);
    payload.push_str(request_json);
    payload.push('\n');
    payload
}

fn extension_shell_fallback_candidates() -> &'static [&'static str] {
    #[cfg(unix)]
    {
        &["/bin/sh", "sh"]
    }
    #[cfg(not(unix))]
    {
        &["sh"]
    }
}

fn run_extension_process_with_timeout(
    entrypoint: &Path,
    request_json: &str,
    timeout_ms: u64,
) -> Result<Output> {
    #[cfg(test)]
    let _guard = extension_process_test_guard();

    let spawn_child = |command: &mut Command| {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
    };
    let mut child = match spawn_child(&mut Command::new(entrypoint)) {
        Ok(child) => child,
        Err(error) => {
            let mut fallback_errors = Vec::new();
            let mut spawned = None;
            for candidate in extension_shell_fallback_candidates() {
                let mut fallback = Command::new(candidate);
                fallback.arg(entrypoint);
                match spawn_child(&mut fallback) {
                    Ok(child) => {
                        spawned = Some(child);
                        break;
                    }
                    Err(candidate_error) => {
                        fallback_errors.push(format!("{candidate}: {candidate_error}"));
                    }
                }
            }
            match spawned {
                Some(child) => child,
                None => {
                    return Err(anyhow!(
                        "failed to spawn extension process {}: {} (fallback attempts failed: {})",
                        entrypoint.display(),
                        error,
                        fallback_errors.join("; ")
                    ));
                }
            }
        }
    };

    {
        let stdin_payload = format_extension_process_stdin_payload(request_json);
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow!("failed to open extension process stdin"))?;
        stdin
            .write_all(stdin_payload.as_bytes())
            .context("failed to write extension payload to process stdin")?;
        stdin
            .flush()
            .context("failed to flush extension payload to process stdin")?;
    }
    child.stdin.take();

    let timeout = Duration::from_millis(timeout_ms);
    if child
        .wait_timeout(timeout)
        .context("failed while waiting for extension process")?
        .is_none()
    {
        let _ = child.kill();
        let _ = child.wait();
        bail!("extension process timed out after {} ms", timeout_ms);
    }

    child
        .wait_with_output()
        .context("failed to collect extension process output")
}

pub fn load_extension_manifest(path: &Path) -> Result<ExtensionManifest> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read extension manifest {}", path.display()))?;
    serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse extension manifest {}", path.display()))
}

fn validate_manifest_schema(manifest: &ExtensionManifest) -> Result<()> {
    if manifest.schema_version != EXTENSION_MANIFEST_SCHEMA_VERSION {
        bail!(
            "unsupported extension manifest schema '{}': expected {}",
            manifest.schema_version,
            EXTENSION_MANIFEST_SCHEMA_VERSION
        );
    }
    Ok(())
}

fn validate_manifest_identifiers(manifest: &ExtensionManifest) -> Result<()> {
    validate_non_empty_field("id", &manifest.id)?;
    validate_non_empty_field("version", &manifest.version)?;
    Ok(())
}

fn validate_non_empty_field(name: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("extension manifest '{}' must not be empty", name);
    }
    Ok(())
}

fn validate_entrypoint_path(entrypoint: &str) -> Result<()> {
    let trimmed = entrypoint.trim();
    if trimmed.is_empty() {
        bail!("extension manifest 'entrypoint' must not be empty");
    }
    let path = Path::new(trimmed);
    if path.is_absolute() {
        bail!(
            "extension manifest entrypoint '{}' must be relative",
            trimmed
        );
    }
    for component in path.components() {
        match component {
            Component::ParentDir => {
                bail!(
                    "extension manifest entrypoint '{}' must not contain parent traversals",
                    trimmed
                );
            }
            Component::Prefix(_) | Component::RootDir => {
                bail!(
                    "extension manifest entrypoint '{}' must be relative",
                    trimmed
                );
            }
            Component::CurDir | Component::Normal(_) => {}
        }
    }
    Ok(())
}

fn validate_unique<T>(entries: &[T], field_name: &str) -> Result<()>
where
    T: Eq + Hash,
{
    let mut seen = HashSet::new();
    for entry in entries {
        if !seen.insert(entry) {
            bail!(
                "extension manifest '{}' contains duplicate entries",
                field_name
            );
        }
    }
    Ok(())
}

fn validate_tool_registrations(tools: &[ExtensionToolRegistration]) -> Result<()> {
    let mut seen = HashSet::new();
    for tool in tools {
        let name = tool.name.trim();
        if name.is_empty() {
            bail!("extension manifest tool name must not be empty");
        }
        if !is_valid_extension_identifier(name) {
            bail!(
                "extension manifest tool '{}' must contain only lowercase alphanumeric, dash, underscore, or dot characters",
                name
            );
        }
        if !seen.insert(name.to_string()) {
            bail!("extension manifest tools contain duplicate name '{}'", name);
        }
        if tool.description.trim().is_empty() {
            bail!(
                "extension manifest tool '{}' description must not be empty",
                name
            );
        }
        validate_tool_parameters_schema(name, &tool.parameters)?;
    }
    Ok(())
}

fn validate_command_registrations(commands: &[ExtensionCommandRegistration]) -> Result<()> {
    let mut seen = HashSet::new();
    for command in commands {
        let normalized = normalize_extension_command_name(&command.name)?;
        if !seen.insert(normalized.clone()) {
            bail!(
                "extension manifest commands contain duplicate name '{}'",
                normalized
            );
        }
        if command.description.trim().is_empty() {
            bail!(
                "extension manifest command '{}' description must not be empty",
                normalized
            );
        }
        if let Some(usage) = command.usage.as_ref() {
            if usage.trim().is_empty() {
                bail!(
                    "extension manifest command '{}' usage must not be empty when set",
                    normalized
                );
            }
        }
    }
    Ok(())
}

fn validate_tool_parameters_schema(name: &str, schema: &Value) -> Result<()> {
    let schema_object = schema.as_object().ok_or_else(|| {
        anyhow!(
            "extension manifest tool '{}' parameters must be a JSON object",
            name
        )
    })?;
    let schema_type = schema_object
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            anyhow!(
                "extension manifest tool '{}' parameters must include string field 'type'",
                name
            )
        })?;
    if schema_type != "object" {
        bail!(
            "extension manifest tool '{}' parameters field 'type' must be 'object'",
            name
        );
    }
    if let Some(properties) = schema_object.get("properties") {
        if !properties.is_object() {
            bail!(
                "extension manifest tool '{}' parameters field 'properties' must be a JSON object",
                name
            );
        }
    }
    if let Some(required) = schema_object.get("required") {
        let required = required.as_array().ok_or_else(|| {
            anyhow!(
                "extension manifest tool '{}' parameters field 'required' must be an array",
                name
            )
        })?;
        if required.iter().any(|entry| match entry.as_str() {
            Some(value) => value.trim().is_empty(),
            None => true,
        }) {
            bail!(
                "extension manifest tool '{}' parameters field 'required' must contain non-empty strings",
                name
            );
        }
    }
    Ok(())
}

fn is_valid_extension_identifier(name: &str) -> bool {
    name.chars().all(|character| {
        character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || character == '-'
            || character == '_'
            || character == '.'
    })
}

fn normalize_extension_command_name(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("extension manifest command name must not be empty");
    }
    let trimmed = trimmed.strip_prefix('/').unwrap_or(trimmed);
    if trimmed.is_empty() {
        bail!("extension manifest command name must not be '/'");
    }
    if trimmed.chars().any(char::is_whitespace) {
        bail!(
            "extension manifest command '{}' must not contain whitespace",
            raw.trim()
        );
    }
    if !is_valid_extension_identifier(trimmed) {
        bail!(
            "extension manifest command '{}' must contain only lowercase alphanumeric, dash, underscore, or dot characters",
            raw.trim()
        );
    }
    Ok(format!("/{}", trimmed))
}

fn validate_timeout_ms(timeout_ms: u64) -> Result<()> {
    if timeout_ms == 0 {
        bail!("extension manifest 'timeout_ms' must be greater than 0");
    }
    if timeout_ms > EXTENSION_TIMEOUT_MS_MAX {
        bail!(
            "extension manifest 'timeout_ms' must be <= {}",
            EXTENSION_TIMEOUT_MS_MAX
        );
    }
    Ok(())
}

fn validate_wasm_runtime_config(manifest: &ExtensionManifest) -> Result<()> {
    if manifest.runtime != ExtensionRuntime::Wasm {
        return Ok(());
    }
    if let Some(fuel_limit) = manifest.wasm.fuel_limit {
        if fuel_limit == 0 {
            bail!("extension manifest wasm 'fuel_limit' must be greater than 0");
        }
    }
    if let Some(memory_limit_bytes) = manifest.wasm.memory_limit_bytes {
        if memory_limit_bytes == 0 {
            bail!("extension manifest wasm 'memory_limit_bytes' must be greater than 0");
        }
    }
    if let Some(max_response_bytes) = manifest.wasm.max_response_bytes {
        if max_response_bytes == 0 {
            bail!("extension manifest wasm 'max_response_bytes' must be greater than 0");
        }
    }
    if manifest
        .wasm
        .env_allowlist
        .iter()
        .any(|name| name.trim().is_empty())
    {
        bail!("extension manifest wasm 'env_allowlist' entries must not be empty");
    }
    Ok(())
}

fn current_unix_timestamp_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests;
