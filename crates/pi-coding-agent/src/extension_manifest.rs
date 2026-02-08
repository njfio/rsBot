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
use wait_timeout::ChildExt;

use crate::Cli;

const EXTENSION_MANIFEST_SCHEMA_VERSION: u32 = 1;
const EXTENSION_TIMEOUT_MS_DEFAULT: u64 = 5_000;
const EXTENSION_TIMEOUT_MS_MAX: u64 = 300_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ExtensionManifestSummary {
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
pub(crate) struct ExtensionListEntry {
    pub manifest_path: PathBuf,
    pub id: String,
    pub version: String,
    pub runtime: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExtensionListInvalidEntry {
    pub manifest_path: PathBuf,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExtensionListReport {
    pub list_root: PathBuf,
    pub entries: Vec<ExtensionListEntry>,
    pub invalid_entries: Vec<ExtensionListInvalidEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExtensionExecSummary {
    pub manifest_path: PathBuf,
    pub id: String,
    pub version: String,
    pub runtime: String,
    pub hook: String,
    pub timeout_ms: u64,
    pub duration_ms: u64,
    pub response_bytes: usize,
    pub response: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExtensionRuntimeHookDispatchSummary {
    pub root: PathBuf,
    pub hook: String,
    pub discovered: usize,
    pub eligible: usize,
    pub executed: usize,
    pub failed: usize,
    pub skipped_invalid: usize,
    pub skipped_unsupported_runtime: usize,
    pub skipped_undeclared_hook: usize,
    pub executed_ids: Vec<String>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExtensionMessageTransformResult {
    pub root: PathBuf,
    pub prompt: String,
    pub executed: usize,
    pub applied: usize,
    pub failed: usize,
    pub skipped_invalid: usize,
    pub skipped_unsupported_runtime: usize,
    pub skipped_undeclared_hook: usize,
    pub applied_ids: Vec<String>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExtensionPolicyOverrideResult {
    pub root: PathBuf,
    pub allowed: bool,
    pub denied_by: Option<String>,
    pub reason: Option<String>,
    pub evaluated: usize,
    pub denied: usize,
    pub skipped_invalid: usize,
    pub skipped_unsupported_runtime: usize,
    pub skipped_undeclared_hook: usize,
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
struct ExtensionManifest {
    schema_version: u32,
    id: String,
    version: String,
    runtime: ExtensionRuntime,
    entrypoint: String,
    #[serde(default)]
    hooks: Vec<ExtensionHook>,
    #[serde(default)]
    permissions: Vec<ExtensionPermission>,
    #[serde(default = "default_extension_timeout_ms")]
    timeout_ms: u64,
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

pub(crate) fn execute_extension_list_command(cli: &Cli) -> Result<()> {
    if !cli.extension_list {
        return Ok(());
    }
    let report = list_extension_manifests(&cli.extension_list_root)?;
    println!("{}", render_extension_list_report(&report));
    Ok(())
}

pub(crate) fn execute_extension_exec_command(cli: &Cli) -> Result<()> {
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
        "extension exec: path={} id={} version={} runtime={} hook={} timeout_ms={} duration_ms={} response_bytes={}",
        summary.manifest_path.display(),
        summary.id,
        summary.version,
        summary.runtime,
        summary.hook,
        summary.timeout_ms,
        summary.duration_ms,
        summary.response_bytes
    );
    println!("extension exec response: {}", summary.response);
    Ok(())
}

pub(crate) fn execute_extension_show_command(cli: &Cli) -> Result<()> {
    let Some(path) = cli.extension_show.as_ref() else {
        return Ok(());
    };
    let (manifest, summary) = load_and_validate_extension_manifest(path)?;
    println!("{}", render_extension_manifest_report(&summary, &manifest));
    Ok(())
}

pub(crate) fn execute_extension_validate_command(cli: &Cli) -> Result<()> {
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

pub(crate) fn validate_extension_manifest(path: &Path) -> Result<ExtensionManifestSummary> {
    let (_, summary) = load_and_validate_extension_manifest(path)?;
    Ok(summary)
}

fn load_and_validate_extension_manifest(
    path: &Path,
) -> Result<(ExtensionManifest, ExtensionManifestSummary)> {
    let manifest = load_extension_manifest(path)?;
    validate_manifest_schema(&manifest)?;
    validate_manifest_identifiers(&manifest)?;
    validate_entrypoint_path(&manifest.entrypoint)?;
    validate_unique(&manifest.hooks, "hooks")?;
    validate_unique(&manifest.permissions, "permissions")?;
    validate_timeout_ms(manifest.timeout_ms)?;
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

fn render_extension_manifest_report(
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
    format!(
        "extension show:\n- path: {}\n- id: {}\n- version: {}\n- runtime: {}\n- entrypoint: {}\n- timeout_ms: {}\n- hooks ({}):\n{}\n- permissions ({}):\n{}",
        summary.manifest_path.display(),
        summary.id,
        summary.version,
        summary.runtime,
        summary.entrypoint,
        summary.timeout_ms,
        summary.hook_count,
        hook_lines,
        summary.permission_count,
        permission_lines
    )
}

fn list_extension_manifests(root: &Path) -> Result<ExtensionListReport> {
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

fn render_extension_list_report(report: &ExtensionListReport) -> String {
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

pub(crate) fn dispatch_extension_runtime_hook(
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
        if loaded_manifest.manifest.runtime != ExtensionRuntime::Process {
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

pub(crate) fn apply_extension_message_transforms(
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
        if loaded_manifest.manifest.runtime != ExtensionRuntime::Process {
            result.skipped_unsupported_runtime += 1;
            continue;
        }
        if !loaded_manifest.manifest.hooks.contains(&hook) {
            result.skipped_undeclared_hook += 1;
            continue;
        }

        result.executed += 1;
        let payload = serde_json::json!({
            "prompt": result.prompt.clone(),
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

pub(crate) fn evaluate_extension_policy_override(
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
        if loaded_manifest.manifest.runtime != ExtensionRuntime::Process {
            result.skipped_unsupported_runtime += 1;
            continue;
        }
        if !loaded_manifest.manifest.hooks.contains(&hook) {
            result.skipped_undeclared_hook += 1;
            continue;
        }

        result.evaluated += 1;
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

fn load_extension_exec_payload(path: &Path) -> Result<Value> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read extension payload {}", path.display()))?;
    let payload = serde_json::from_str::<Value>(&raw)
        .with_context(|| format!("failed to parse extension payload {}", path.display()))?;
    if !payload.is_object() {
        bail!("extension payload must be a JSON object");
    }
    Ok(payload)
}

fn execute_extension_process_hook(
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
    if manifest.runtime != ExtensionRuntime::Process {
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
    let started_at = Instant::now();
    let output =
        run_extension_process_with_timeout(&entrypoint, &request_json, manifest.timeout_ms)?;
    let duration_ms = started_at.elapsed().as_millis() as u64;
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
    let response_raw =
        String::from_utf8(output.stdout).context("extension process output is not valid UTF-8")?;
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

    Ok(ExtensionExecSummary {
        manifest_path: summary.manifest_path.clone(),
        id: summary.id.clone(),
        version: summary.version.clone(),
        runtime: summary.runtime.clone(),
        hook: hook.as_str().to_string(),
        timeout_ms: manifest.timeout_ms,
        duration_ms,
        response_bytes: response.len(),
        response,
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

fn run_extension_process_with_timeout(
    entrypoint: &Path,
    request_json: &str,
    timeout_ms: u64,
) -> Result<Output> {
    let mut child = Command::new(entrypoint)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn extension process {}", entrypoint.display()))?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow!("failed to open extension process stdin"))?;
        stdin
            .write_all(request_json.as_bytes())
            .context("failed to write extension payload to process stdin")?;
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

fn load_extension_manifest(path: &Path) -> Result<ExtensionManifest> {
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

#[cfg(test)]
mod tests {
    use super::{
        apply_extension_message_transforms, dispatch_extension_runtime_hook,
        evaluate_extension_policy_override, execute_extension_process_hook,
        list_extension_manifests, parse_message_transform_response_prompt,
        parse_policy_override_response, render_extension_list_report,
        render_extension_manifest_report, validate_extension_manifest, ExtensionHook,
        ExtensionListReport, ExtensionManifest, ExtensionManifestSummary, ExtensionPermission,
        ExtensionRuntime, PolicyOverrideDecision,
    };
    use std::{fs, path::PathBuf};
    use tempfile::tempdir;

    #[test]
    fn unit_validate_extension_manifest_accepts_minimal_schema() {
        let temp = tempdir().expect("tempdir");
        let manifest_path = temp.path().join("extension.json");
        std::fs::write(
            &manifest_path,
            r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/assistant"
}"#,
        )
        .expect("write manifest");

        let summary = validate_extension_manifest(&manifest_path).expect("valid manifest");
        assert_eq!(summary.id, "issue-assistant");
        assert_eq!(summary.version, "0.1.0");
        assert_eq!(summary.runtime, "process");
        assert_eq!(summary.entrypoint, "bin/assistant");
        assert_eq!(summary.hook_count, 0);
        assert_eq!(summary.permission_count, 0);
        assert_eq!(summary.timeout_ms, 5_000);
    }

    #[test]
    fn regression_validate_extension_manifest_rejects_parent_dir_entrypoint() {
        let temp = tempdir().expect("tempdir");
        let manifest_path = temp.path().join("extension.json");
        std::fs::write(
            &manifest_path,
            r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "../escape.sh"
}"#,
        )
        .expect("write manifest");

        let error =
            validate_extension_manifest(&manifest_path).expect_err("parent traversal should fail");
        assert!(error
            .to_string()
            .contains("must not contain parent traversals"));
    }

    #[test]
    fn regression_validate_extension_manifest_rejects_duplicate_hooks() {
        let temp = tempdir().expect("tempdir");
        let manifest_path = temp.path().join("extension.json");
        std::fs::write(
            &manifest_path,
            r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/assistant",
  "hooks": ["run-start", "run-start"]
}"#,
        )
        .expect("write manifest");

        let error =
            validate_extension_manifest(&manifest_path).expect_err("duplicate hooks should fail");
        assert!(error.to_string().contains("contains duplicate entries"));
    }

    #[test]
    fn unit_render_extension_manifest_report_is_deterministic() {
        let summary = ExtensionManifestSummary {
            manifest_path: PathBuf::from("extensions/issue-assistant/extension.json"),
            id: "issue-assistant".to_string(),
            version: "0.1.0".to_string(),
            runtime: "process".to_string(),
            entrypoint: "bin/assistant".to_string(),
            hook_count: 2,
            permission_count: 2,
            timeout_ms: 60_000,
        };
        let manifest = ExtensionManifest {
            schema_version: 1,
            id: "issue-assistant".to_string(),
            version: "0.1.0".to_string(),
            runtime: ExtensionRuntime::Process,
            entrypoint: "bin/assistant".to_string(),
            hooks: vec![ExtensionHook::RunStart, ExtensionHook::RunEnd],
            permissions: vec![ExtensionPermission::Network, ExtensionPermission::ReadFiles],
            timeout_ms: 60_000,
        };

        let report = render_extension_manifest_report(&summary, &manifest);
        assert!(report.contains("extension show:"));
        assert!(report.contains("- id: issue-assistant"));
        assert!(report.contains("- hooks (2):\n- run-end\n- run-start"));
        assert!(report.contains("- permissions (2):\n- network\n- read-files"));
    }

    #[test]
    fn unit_render_extension_list_report_is_deterministic() {
        let report = ExtensionListReport {
            list_root: PathBuf::from("extensions"),
            entries: vec![super::ExtensionListEntry {
                manifest_path: PathBuf::from("extensions/issue-assistant/extension.json"),
                id: "issue-assistant".to_string(),
                version: "0.1.0".to_string(),
                runtime: "process".to_string(),
            }],
            invalid_entries: vec![super::ExtensionListInvalidEntry {
                manifest_path: PathBuf::from("extensions/bad/extension.json"),
                error: "unsupported extension manifest schema".to_string(),
            }],
        };

        let rendered = render_extension_list_report(&report);
        assert!(rendered.contains("extension list: root=extensions count=1 invalid=1"));
        assert!(rendered.contains(
            "extension: id=issue-assistant version=0.1.0 runtime=process manifest=extensions/issue-assistant/extension.json"
        ));
        assert!(rendered.contains("invalid: manifest=extensions/bad/extension.json error=unsupported extension manifest schema"));
    }

    #[test]
    fn regression_list_extension_manifests_reports_invalid_entries_without_failing() {
        let temp = tempdir().expect("tempdir");
        let good_dir = temp.path().join("good");
        fs::create_dir_all(&good_dir).expect("create good dir");
        fs::write(
            good_dir.join("extension.json"),
            r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/assistant"
}"#,
        )
        .expect("write valid extension");

        let bad_dir = temp.path().join("bad");
        fs::create_dir_all(&bad_dir).expect("create bad dir");
        fs::write(
            bad_dir.join("extension.json"),
            r#"{
  "schema_version": 9,
  "id": "broken",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/assistant"
}"#,
        )
        .expect("write invalid extension");

        let report = list_extension_manifests(temp.path()).expect("list should succeed");
        assert_eq!(report.entries.len(), 1);
        assert_eq!(report.invalid_entries.len(), 1);
        assert_eq!(report.entries[0].id, "issue-assistant");
        assert!(report.invalid_entries[0]
            .error
            .contains("unsupported extension manifest schema"));
    }

    #[test]
    fn regression_list_extension_manifests_rejects_non_directory_root() {
        let temp = tempdir().expect("tempdir");
        let root_file = temp.path().join("extensions.json");
        fs::write(&root_file, "{}").expect("write root file");

        let error =
            list_extension_manifests(&root_file).expect_err("non-directory root should fail");
        assert!(error.to_string().contains("is not a directory"));
    }

    fn make_executable(path: &std::path::Path) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(path).expect("metadata").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(path, permissions).expect("set executable permissions");
        }
    }

    #[test]
    fn functional_execute_extension_process_hook_runs_process_runtime() {
        let temp = tempdir().expect("tempdir");
        let script_path = temp.path().join("hook.sh");
        fs::write(
            &script_path,
            "#!/bin/sh\nread -r _input\nprintf '{\"ok\":true,\"result\":\"hook-processed\"}'\n",
        )
        .expect("write script");
        make_executable(&script_path);

        let manifest_path = temp.path().join("extension.json");
        fs::write(
            &manifest_path,
            r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "hook.sh",
  "hooks": ["run-start"],
  "timeout_ms": 5000
}"#,
        )
        .expect("write manifest");

        let payload = serde_json::json!({"event":"created"});
        let summary = execute_extension_process_hook(&manifest_path, "run-start", &payload)
            .expect("extension execution should succeed");
        assert_eq!(summary.id, "issue-assistant");
        assert_eq!(summary.hook, "run-start");
        assert!(summary.response.contains("\"ok\":true"));
    }

    #[test]
    fn regression_execute_extension_process_hook_rejects_undeclared_hook() {
        let temp = tempdir().expect("tempdir");
        let script_path = temp.path().join("hook.sh");
        fs::write(
            &script_path,
            "#!/bin/sh\nread -r _input\nprintf '{\"ok\":true}'\n",
        )
        .expect("write script");
        make_executable(&script_path);

        let manifest_path = temp.path().join("extension.json");
        fs::write(
            &manifest_path,
            r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "hook.sh",
  "hooks": ["run-end"],
  "timeout_ms": 5000
}"#,
        )
        .expect("write manifest");

        let payload = serde_json::json!({"event":"created"});
        let error = execute_extension_process_hook(&manifest_path, "run-start", &payload)
            .expect_err("undeclared hook should fail");
        assert!(error.to_string().contains("does not declare hook"));
    }

    #[test]
    fn regression_execute_extension_process_hook_enforces_timeout() {
        let temp = tempdir().expect("tempdir");
        let script_path = temp.path().join("slow.sh");
        fs::write(&script_path, "#!/bin/sh\nsleep 1\nprintf '{\"ok\":true}'\n")
            .expect("write script");
        make_executable(&script_path);

        let manifest_path = temp.path().join("extension.json");
        fs::write(
            &manifest_path,
            r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "slow.sh",
  "hooks": ["run-start"],
  "timeout_ms": 20
}"#,
        )
        .expect("write manifest");

        let payload = serde_json::json!({"event":"created"});
        let error = execute_extension_process_hook(&manifest_path, "run-start", &payload)
            .expect_err("timeout should fail");
        assert!(error.to_string().contains("timed out"));
    }

    #[test]
    fn regression_execute_extension_process_hook_rejects_invalid_json_output() {
        let temp = tempdir().expect("tempdir");
        let script_path = temp.path().join("bad-output.sh");
        fs::write(
            &script_path,
            "#!/bin/sh\nread -r _input\nprintf 'not-json'\n",
        )
        .expect("write script");
        make_executable(&script_path);

        let manifest_path = temp.path().join("extension.json");
        fs::write(
            &manifest_path,
            r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bad-output.sh",
  "hooks": ["run-start"],
  "timeout_ms": 5000
}"#,
        )
        .expect("write manifest");

        let payload = serde_json::json!({"event":"created"});
        let error = execute_extension_process_hook(&manifest_path, "run-start", &payload)
            .expect_err("invalid output should fail");
        assert!(error.to_string().contains("response must be valid JSON"));
    }

    #[test]
    fn unit_dispatch_extension_runtime_hook_orders_execution_deterministically() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("extensions");
        let alpha_dir = root.join("alpha");
        let beta_dir = root.join("beta");
        fs::create_dir_all(&alpha_dir).expect("create alpha dir");
        fs::create_dir_all(&beta_dir).expect("create beta dir");

        let alpha_script = alpha_dir.join("hook.sh");
        fs::write(
            &alpha_script,
            "#!/bin/sh\ncat >/dev/null\nprintf '{\"ok\":true}'\n",
        )
        .expect("write alpha script");
        make_executable(&alpha_script);

        let beta_script = beta_dir.join("hook.sh");
        fs::write(
            &beta_script,
            "#!/bin/sh\ncat >/dev/null\nprintf '{\"ok\":true}'\n",
        )
        .expect("write beta script");
        make_executable(&beta_script);

        fs::write(
            alpha_dir.join("extension.json"),
            r#"{
  "schema_version": 1,
  "id": "aaa-extension",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "hook.sh",
  "hooks": ["run-start"],
  "timeout_ms": 5000
}"#,
        )
        .expect("write alpha manifest");
        fs::write(
            beta_dir.join("extension.json"),
            r#"{
  "schema_version": 1,
  "id": "zzz-extension",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "hook.sh",
  "hooks": ["run-start"],
  "timeout_ms": 5000
}"#,
        )
        .expect("write beta manifest");

        let report = dispatch_extension_runtime_hook(&root, "run-start", &serde_json::json!({}));
        assert_eq!(report.discovered, 2);
        assert_eq!(report.executed, 2);
        assert_eq!(
            report.executed_ids,
            vec![
                "aaa-extension@1.0.0".to_string(),
                "zzz-extension@1.0.0".to_string()
            ]
        );
    }

    #[test]
    fn functional_dispatch_extension_runtime_hook_runs_process_extensions() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("extensions");
        let extension_dir = root.join("issue-assistant");
        fs::create_dir_all(&extension_dir).expect("create extension dir");

        let script_path = extension_dir.join("hook.sh");
        fs::write(
            &script_path,
            "#!/bin/sh\ncat >/dev/null\nprintf '{\"ok\":true}'\n",
        )
        .expect("write script");
        make_executable(&script_path);

        fs::write(
            extension_dir.join("extension.json"),
            r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "hook.sh",
  "hooks": ["run-start", "run-end"],
  "timeout_ms": 5000
}"#,
        )
        .expect("write manifest");

        let report = dispatch_extension_runtime_hook(
            &root,
            "run-start",
            &serde_json::json!({"event":"started"}),
        );
        assert_eq!(report.executed, 1);
        assert_eq!(report.failed, 0);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn regression_dispatch_extension_runtime_hook_isolates_failures() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("extensions");
        let good_dir = root.join("good");
        let bad_dir = root.join("bad");
        fs::create_dir_all(&good_dir).expect("create good dir");
        fs::create_dir_all(&bad_dir).expect("create bad dir");

        let good_script = good_dir.join("hook.sh");
        fs::write(
            &good_script,
            "#!/bin/sh\ncat >/dev/null\nprintf '{\"ok\":true}'\n",
        )
        .expect("write good script");
        make_executable(&good_script);

        let bad_script = bad_dir.join("slow.sh");
        fs::write(&bad_script, "#!/bin/sh\nsleep 1\nprintf '{\"ok\":true}'\n")
            .expect("write bad script");
        make_executable(&bad_script);

        fs::write(
            good_dir.join("extension.json"),
            r#"{
  "schema_version": 1,
  "id": "good-extension",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "hook.sh",
  "hooks": ["run-start"],
  "timeout_ms": 5000
}"#,
        )
        .expect("write good manifest");
        fs::write(
            bad_dir.join("extension.json"),
            r#"{
  "schema_version": 1,
  "id": "bad-extension",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "slow.sh",
  "hooks": ["run-start"],
  "timeout_ms": 20
}"#,
        )
        .expect("write bad manifest");

        let report = dispatch_extension_runtime_hook(&root, "run-start", &serde_json::json!({}));
        assert_eq!(report.discovered, 2);
        assert_eq!(report.executed, 1);
        assert_eq!(report.failed, 1);
        assert!(report
            .diagnostics
            .iter()
            .any(|line| line.contains("timed out")));
    }

    #[test]
    fn regression_dispatch_extension_runtime_hook_skips_invalid_manifests() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("extensions");
        let valid_dir = root.join("valid");
        let invalid_dir = root.join("invalid");
        fs::create_dir_all(&valid_dir).expect("create valid dir");
        fs::create_dir_all(&invalid_dir).expect("create invalid dir");

        let script_path = valid_dir.join("hook.sh");
        fs::write(
            &script_path,
            "#!/bin/sh\ncat >/dev/null\nprintf '{\"ok\":true}'\n",
        )
        .expect("write script");
        make_executable(&script_path);

        fs::write(
            valid_dir.join("extension.json"),
            r#"{
  "schema_version": 1,
  "id": "valid-extension",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "hook.sh",
  "hooks": ["run-start"]
}"#,
        )
        .expect("write valid manifest");
        fs::write(
            invalid_dir.join("extension.json"),
            r#"{
  "schema_version": 9,
  "id": "invalid-extension",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "hook.sh"
}"#,
        )
        .expect("write invalid manifest");

        let report = dispatch_extension_runtime_hook(&root, "run-start", &serde_json::json!({}));
        assert_eq!(report.executed, 1);
        assert_eq!(report.skipped_invalid, 1);
        assert!(report
            .diagnostics
            .iter()
            .any(|line| line.contains("skipped invalid manifest")));
    }

    #[test]
    fn unit_parse_message_transform_response_prompt_accepts_valid_prompt() {
        let prompt =
            parse_message_transform_response_prompt(r#"{"prompt":"refined prompt"}"#).expect("ok");
        assert_eq!(prompt.as_deref(), Some("refined prompt"));
    }

    #[test]
    fn regression_parse_message_transform_response_prompt_rejects_non_string_prompt() {
        let error = parse_message_transform_response_prompt(r#"{"prompt":42}"#)
            .expect_err("non-string prompt should fail");
        assert!(error.to_string().contains("must be a string"));
    }

    #[test]
    fn functional_apply_extension_message_transforms_rewrites_prompt() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("extensions");
        let extension_dir = root.join("transformer");
        fs::create_dir_all(&extension_dir).expect("create extension dir");

        let script_path = extension_dir.join("transform.sh");
        fs::write(
            &script_path,
            "#!/bin/sh\ncat >/dev/null\nprintf '{\"prompt\":\"rewritten prompt\"}'\n",
        )
        .expect("write script");
        make_executable(&script_path);

        fs::write(
            extension_dir.join("extension.json"),
            r#"{
  "schema_version": 1,
  "id": "transformer",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "transform.sh",
  "hooks": ["message-transform"],
  "timeout_ms": 5000
}"#,
        )
        .expect("write manifest");

        let result = apply_extension_message_transforms(&root, "original prompt");
        assert_eq!(result.prompt, "rewritten prompt");
        assert_eq!(result.executed, 1);
        assert_eq!(result.applied, 1);
        assert_eq!(result.applied_ids, vec!["transformer@0.1.0".to_string()]);
    }

    #[test]
    fn integration_apply_extension_message_transforms_applies_in_deterministic_order() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("extensions");
        let a_dir = root.join("a");
        let b_dir = root.join("b");
        fs::create_dir_all(&a_dir).expect("create a dir");
        fs::create_dir_all(&b_dir).expect("create b dir");

        let a_script = a_dir.join("transform.sh");
        fs::write(
            &a_script,
            "#!/bin/sh\ncat >/dev/null\nprintf '{\"prompt\":\"alpha\"}'\n",
        )
        .expect("write a script");
        make_executable(&a_script);
        let b_script = b_dir.join("transform.sh");
        fs::write(
            &b_script,
            "#!/bin/sh\ncat >/dev/null\nprintf '{\"prompt\":\"beta\"}'\n",
        )
        .expect("write b script");
        make_executable(&b_script);

        fs::write(
            a_dir.join("extension.json"),
            r#"{
  "schema_version": 1,
  "id": "a-extension",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "transform.sh",
  "hooks": ["message-transform"],
  "timeout_ms": 5000
}"#,
        )
        .expect("write a manifest");
        fs::write(
            b_dir.join("extension.json"),
            r#"{
  "schema_version": 1,
  "id": "b-extension",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "transform.sh",
  "hooks": ["message-transform"],
  "timeout_ms": 5000
}"#,
        )
        .expect("write b manifest");

        let result = apply_extension_message_transforms(&root, "seed");
        assert_eq!(result.prompt, "beta");
        assert_eq!(result.applied, 2);
        assert_eq!(
            result.applied_ids,
            vec![
                "a-extension@1.0.0".to_string(),
                "b-extension@1.0.0".to_string()
            ]
        );
    }

    #[test]
    fn regression_apply_extension_message_transforms_falls_back_on_invalid_output() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("extensions");
        let extension_dir = root.join("broken-transformer");
        fs::create_dir_all(&extension_dir).expect("create extension dir");

        let script_path = extension_dir.join("transform.sh");
        fs::write(
            &script_path,
            "#!/bin/sh\ncat >/dev/null\nprintf '{\"prompt\":123}'\n",
        )
        .expect("write script");
        make_executable(&script_path);

        fs::write(
            extension_dir.join("extension.json"),
            r#"{
  "schema_version": 1,
  "id": "broken-transformer",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "transform.sh",
  "hooks": ["message-transform"],
  "timeout_ms": 5000
}"#,
        )
        .expect("write manifest");

        let result = apply_extension_message_transforms(&root, "original prompt");
        assert_eq!(result.prompt, "original prompt");
        assert_eq!(result.executed, 1);
        assert_eq!(result.applied, 0);
        assert!(result
            .diagnostics
            .iter()
            .any(|line| line.contains("must be a string")));
    }

    #[test]
    fn unit_parse_policy_override_response_accepts_allow_decision() {
        let response =
            parse_policy_override_response(r#"{"decision":"allow"}"#).expect("response parses");
        assert_eq!(response.decision, PolicyOverrideDecision::Allow);
        assert_eq!(response.reason, None);
    }

    #[test]
    fn unit_parse_policy_override_response_accepts_deny_decision_with_reason() {
        let response = parse_policy_override_response(r#"{"decision":"deny","reason":"blocked"}"#)
            .expect("response parses");
        assert_eq!(response.decision, PolicyOverrideDecision::Deny);
        assert_eq!(response.reason.as_deref(), Some("blocked"));
    }

    #[test]
    fn regression_parse_policy_override_response_rejects_invalid_decision() {
        let error = parse_policy_override_response(r#"{"decision":"defer"}"#)
            .expect_err("invalid decision should fail");
        assert!(error.to_string().contains("must be 'allow' or 'deny'"));
    }

    #[test]
    fn functional_evaluate_extension_policy_override_denies_command() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("extensions");
        let extension_dir = root.join("policy-enforcer");
        fs::create_dir_all(&extension_dir).expect("create extension dir");

        let script_path = extension_dir.join("policy.sh");
        fs::write(
            &script_path,
            "#!/bin/sh\ncat >/dev/null\nprintf '{\"decision\":\"deny\",\"reason\":\"blocked by extension\"}'\n",
        )
        .expect("write script");
        make_executable(&script_path);

        fs::write(
            extension_dir.join("extension.json"),
            r#"{
  "schema_version": 1,
  "id": "policy-enforcer",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "policy.sh",
  "hooks": ["policy-override"],
  "timeout_ms": 5000
}"#,
        )
        .expect("write manifest");

        let result = evaluate_extension_policy_override(
            &root,
            &serde_json::json!({"command":"printf 'ok'","tool":"bash"}),
        );
        assert!(!result.allowed);
        assert_eq!(result.denied, 1);
        assert_eq!(result.evaluated, 1);
        assert_eq!(result.denied_by.as_deref(), Some("policy-enforcer@1.0.0"));
        assert_eq!(result.reason.as_deref(), Some("blocked by extension"));
    }

    #[test]
    fn integration_evaluate_extension_policy_override_allows_command() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("extensions");
        let extension_dir = root.join("policy-enforcer");
        fs::create_dir_all(&extension_dir).expect("create extension dir");

        let script_path = extension_dir.join("policy.sh");
        fs::write(
            &script_path,
            "#!/bin/sh\ncat >/dev/null\nprintf '{\"decision\":\"allow\"}'\n",
        )
        .expect("write script");
        make_executable(&script_path);

        fs::write(
            extension_dir.join("extension.json"),
            r#"{
  "schema_version": 1,
  "id": "policy-enforcer",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "policy.sh",
  "hooks": ["policy-override"],
  "timeout_ms": 5000
}"#,
        )
        .expect("write manifest");

        let result = evaluate_extension_policy_override(
            &root,
            &serde_json::json!({"command":"printf 'ok'","tool":"bash"}),
        );
        assert!(result.allowed);
        assert_eq!(result.denied, 0);
        assert_eq!(result.evaluated, 1);
        assert_eq!(result.reason, None);
    }

    #[test]
    fn regression_evaluate_extension_policy_override_fails_closed_on_invalid_response() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("extensions");
        let extension_dir = root.join("broken-policy");
        fs::create_dir_all(&extension_dir).expect("create extension dir");

        let script_path = extension_dir.join("policy.sh");
        fs::write(
            &script_path,
            "#!/bin/sh\ncat >/dev/null\nprintf '{\"decision\":123}'\n",
        )
        .expect("write script");
        make_executable(&script_path);

        fs::write(
            extension_dir.join("extension.json"),
            r#"{
  "schema_version": 1,
  "id": "broken-policy",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "policy.sh",
  "hooks": ["policy-override"],
  "timeout_ms": 5000
}"#,
        )
        .expect("write manifest");

        let result = evaluate_extension_policy_override(
            &root,
            &serde_json::json!({"command":"printf 'ok'","tool":"bash"}),
        );
        assert!(!result.allowed);
        assert_eq!(result.denied, 1);
        assert_eq!(result.denied_by.as_deref(), Some("broken-policy@1.0.0"));
        assert!(result
            .reason
            .as_deref()
            .unwrap_or_default()
            .contains("invalid response"));
        assert!(result
            .diagnostics
            .iter()
            .any(|line| line.contains("invalid response")));
    }
}
