use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    ffi::OsString,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

use async_trait::async_trait;
use reqwest::{
    header::{HeaderName, HeaderValue},
    redirect::Policy as RedirectPolicy,
    Method, StatusCode,
};
use serde::Serialize;
use serde_json::{json, Value};
use tau_agent_core::{Agent, AgentTool, DefaultLeakDetector, LeakDetector, ToolExecutionResult};
use tau_ai::{Message, ToolDefinition};
use tau_extensions::{execute_extension_registered_tool, ExtensionRegisteredTool};
use tau_runtime::{
    build_generated_wasm_tool, BackgroundJobCreateRequest, BackgroundJobRuntime,
    BackgroundJobRuntimeConfig, BackgroundJobStatusFilter, BackgroundJobTraceContext,
    GeneratedToolBuildRequest, SsrfGuard, SsrfProtectionConfig, SsrfViolation,
    WasmSandboxCapabilityProfile, WasmSandboxFilesystemMode, WasmSandboxLimits,
    WasmSandboxNetworkMode, WASM_SANDBOX_FUEL_LIMIT_DEFAULT,
    WASM_SANDBOX_MAX_RESPONSE_BYTES_DEFAULT, WASM_SANDBOX_MEMORY_LIMIT_BYTES_DEFAULT,
    WASM_SANDBOX_TIMEOUT_MS_DEFAULT,
};

use tau_access::ApprovalAction;
use tau_memory::memory_contract::{MemoryEntry, MemoryScope};
use tau_memory::runtime::{
    FileMemoryStore, MemoryEmbeddingProviderConfig, MemoryRelationInput, MemoryRelationType,
    MemoryScopeFilter, MemorySearchOptions, MemoryType, MemoryTypeImportanceProfile,
};
use tau_session::{
    redo_session_head, resolve_session_navigation_head, session_message_preview, undo_session_head,
    SessionRuntime, SessionStore,
};

const BALANCED_COMMAND_ALLOWLIST: &[&str] = &[
    "awk", "cargo", "cat", "cp", "cut", "du", "echo", "env", "fd", "find", "git", "grep", "head",
    "ls", "mkdir", "mv", "printf", "pwd", "rg", "rm", "rustc", "rustup", "sed", "sleep", "sort",
    "stat", "tail", "touch", "tr", "uniq", "wc",
];

const STRICT_COMMAND_ALLOWLIST: &[&str] = &[
    "awk", "cat", "cut", "du", "echo", "env", "fd", "find", "grep", "head", "ls", "printf", "pwd",
    "rg", "sed", "sort", "stat", "tail", "tr", "uniq", "wc",
];

const MEMORY_SEARCH_DEFAULT_LIMIT: usize = 5;
const MEMORY_SEARCH_MAX_LIMIT: usize = 50;
const MEMORY_WRITE_MAX_SUMMARY_CHARS: usize = 1_200;
const MEMORY_WRITE_MAX_FACTS: usize = 32;
const MEMORY_WRITE_MAX_TAGS: usize = 32;
const MEMORY_WRITE_MAX_FACT_CHARS: usize = 400;
const MEMORY_WRITE_MAX_TAG_CHARS: usize = 96;
const MEMORY_EMBEDDING_TIMEOUT_MS_DEFAULT: u64 = 10_000;
const MEMORY_BM25_K1_DEFAULT: f32 = 1.2;
const MEMORY_BM25_B_DEFAULT: f32 = 0.75;
const MEMORY_BM25_MIN_SCORE_DEFAULT: f32 = 0.0;
const MEMORY_RRF_K_DEFAULT: usize = 60;
const MEMORY_RRF_VECTOR_WEIGHT_DEFAULT: f32 = 1.0;
const MEMORY_RRF_LEXICAL_WEIGHT_DEFAULT: f32 = 1.0;
const JOBS_LIST_DEFAULT_LIMIT: usize = 20;
const JOBS_LIST_MAX_LIMIT: usize = 200;
const JOBS_DEFAULT_TIMEOUT_MS: u64 = 30_000;
const JOBS_MAX_TIMEOUT_MS: u64 = 900_000;
const JOBS_OUTPUT_PREVIEW_DEFAULT_BYTES: usize = 2_000;
const JOBS_OUTPUT_PREVIEW_MAX_BYTES: usize = 16_000;
const BRANCH_TOOL_MAX_PROMPT_CHARS: usize = 4_000;
const TOOL_RATE_LIMIT_WINDOW_MS_DEFAULT: u64 = 60_000;
const TOOL_RATE_LIMIT_MAX_REQUESTS_PERMISSIVE: u32 = 240;
const TOOL_RATE_LIMIT_MAX_REQUESTS_BALANCED: u32 = 120;
const TOOL_RATE_LIMIT_MAX_REQUESTS_STRICT: u32 = 60;
const TOOL_RATE_LIMIT_MAX_REQUESTS_HARDENED: u32 = 30;
const TOOL_BUILDER_MAX_ATTEMPTS_DEFAULT: usize = 3;
const TOOL_BUILDER_MAX_ATTEMPTS_MAX: usize = 8;
const TOOL_HTTP_TIMEOUT_MS_PERMISSIVE: u64 = 60_000;
const TOOL_HTTP_TIMEOUT_MS_BALANCED: u64 = 20_000;
const TOOL_HTTP_TIMEOUT_MS_STRICT: u64 = 15_000;
const TOOL_HTTP_TIMEOUT_MS_HARDENED: u64 = 10_000;
const TOOL_HTTP_MAX_RESPONSE_BYTES_PERMISSIVE: usize = 1_000_000;
const TOOL_HTTP_MAX_RESPONSE_BYTES_BALANCED: usize = 256_000;
const TOOL_HTTP_MAX_RESPONSE_BYTES_STRICT: usize = 128_000;
const TOOL_HTTP_MAX_RESPONSE_BYTES_HARDENED: usize = 64_000;
const TOOL_HTTP_MAX_REDIRECTS_PERMISSIVE: usize = 8;
const TOOL_HTTP_MAX_REDIRECTS_BALANCED: usize = 5;
const TOOL_HTTP_MAX_REDIRECTS_STRICT: usize = 3;
const TOOL_HTTP_MAX_REDIRECTS_HARDENED: usize = 2;
const DOCKER_SANDBOX_DEFAULT_IMAGE: &str = "debian:stable-slim";
const DOCKER_SANDBOX_DEFAULT_MEMORY_MB: u64 = 256;
const DOCKER_SANDBOX_DEFAULT_CPUS: f32 = 1.0;
const DOCKER_SANDBOX_DEFAULT_PIDS_LIMIT: u64 = 256;
const DOCKER_SANDBOX_TMPFS_SIZE_MB: u64 = 64;
const SANDBOX_REQUIRED_UNAVAILABLE_ERROR: &str =
    "OS sandbox policy mode 'required' is enabled but command would run without a sandbox launcher";
const SANDBOX_FORCE_UNAVAILABLE_ERROR: &str =
    "OS sandbox mode 'force' is enabled but no sandbox launcher is configured or available";
const SANDBOX_DOCKER_UNAVAILABLE_ERROR: &str =
    "OS sandbox Docker backend is enabled but Docker CLI is unavailable";
static MEMORY_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
static BACKGROUND_JOB_RUNTIME_REGISTRY: OnceLock<
    Mutex<HashMap<PathBuf, Arc<BackgroundJobRuntime>>>,
> = OnceLock::new();
const DEFAULT_PROTECTED_RELATIVE_PATHS: &[&str] = &[
    "AGENTS.md",
    "SOUL.md",
    "USER.md",
    ".tau/AGENTS.md",
    ".tau/SOUL.md",
    ".tau/USER.md",
    ".tau/rbac-policy.json",
    ".tau/trust-roots.json",
    ".tau/channel-policy.json",
];
const BUILTIN_AGENT_TOOL_NAMES: &[&str] = &[
    "read",
    "write",
    "edit",
    "memory_write",
    "memory_read",
    "memory_delete",
    "memory_search",
    "memory_tree",
    "sessions_list",
    "sessions_history",
    "sessions_search",
    "sessions_stats",
    "sessions_send",
    "jobs_create",
    "jobs_list",
    "jobs_status",
    "jobs_cancel",
    "branch",
    "undo",
    "redo",
    "send_file",
    "react",
    "skip",
    "http",
    "tool_builder",
    "bash",
];

mod bash_tool;
mod jobs_tools;
mod memory_tools;
mod registry_core;
mod runtime_helpers;
mod session_tools;

pub use bash_tool::BashTool;
use bash_tool::{
    evaluate_tool_approval_gate, evaluate_tool_rate_limit_gate, evaluate_tool_rbac_gate,
};
pub use jobs_tools::{JobsCancelTool, JobsCreateTool, JobsListTool, JobsStatusTool};
pub use memory_tools::{
    MemoryDeleteTool, MemoryReadTool, MemorySearchTool, MemoryTreeTool, MemoryWriteTool,
};
use registry_core::BashSandboxSpec;
pub use registry_core::{
    builtin_agent_tool_names, register_builtin_tools, register_extension_tools,
    tool_policy_preset_name, tool_rate_limit_behavior_name, BashCommandProfile,
    OsSandboxDockerNetwork, OsSandboxMode, OsSandboxPolicyMode, ToolPolicy, ToolPolicyPreset,
    ToolRateLimitCounters, ToolRateLimitExceededBehavior,
};
use runtime_helpers::*;
pub use runtime_helpers::{os_sandbox_docker_network_name, os_sandbox_policy_mode_name};
#[cfg(test)]
use session_tools::is_session_candidate_path;
pub use session_tools::{
    SessionsHistoryTool, SessionsListTool, SessionsSearchTool, SessionsSendTool, SessionsStatsTool,
};

/// Public struct `ToolBuilderTool` used across Tau components.
pub struct ToolBuilderTool {
    policy: Arc<ToolPolicy>,
}

impl ToolBuilderTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl AgentTool for ToolBuilderTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "tool_builder".to_string(),
            description: "Generate, compile, persist, and register a wasm-backed extension tool"
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Generated tool name (lowercase alphanumeric, dash, underscore)" },
                    "description": { "type": "string", "description": "Generated tool description" },
                    "spec": { "type": "string", "description": "Natural-language tool behavior specification" },
                    "parameters": { "type": "object", "description": "JSON schema for generated tool arguments" },
                    "wat_source": { "type": "string", "description": "Optional initial WAT source candidate" },
                    "max_attempts": { "type": "integer", "minimum": 1, "maximum": 8 },
                    "timeout_ms": { "type": "integer", "minimum": 1 },
                    "fuel_limit": { "type": "integer", "minimum": 1 },
                    "memory_limit_bytes": { "type": "integer", "minimum": 1 },
                    "max_response_bytes": { "type": "integer", "minimum": 1 },
                    "filesystem_mode": { "type": "string", "enum": ["deny", "read-only", "read-write"] },
                    "network_mode": { "type": "string", "enum": ["deny", "allow"] },
                    "env_allowlist": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "output_root": { "type": "string", "description": "Optional override for generated artifact root" },
                    "extension_root": { "type": "string", "description": "Optional override for generated extension registration root" }
                },
                "required": ["name", "description", "spec"],
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        if !self.policy.tool_builder_enabled {
            return ToolExecutionResult::error(json!({
                "error": "tool_builder is disabled by policy",
                "reason_code": "tool_builder_disabled",
            }));
        }

        let name = match required_string(&arguments, "name") {
            Ok(value) => value,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "error": error,
                    "reason_code": "tool_builder_invalid_name",
                }));
            }
        };
        let description = match required_string(&arguments, "description") {
            Ok(value) => value,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "error": error,
                    "reason_code": "tool_builder_invalid_description",
                }));
            }
        };
        let spec = match required_string(&arguments, "spec") {
            Ok(value) => value,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "error": error,
                    "reason_code": "tool_builder_invalid_spec",
                }));
            }
        };
        let parameters = arguments.get("parameters").cloned().unwrap_or_else(
            || json!({"type":"object","properties":{},"additionalProperties":false}),
        );
        if !parameters.is_object() {
            return ToolExecutionResult::error(json!({
                "error": "field 'parameters' must be a JSON object",
                "reason_code": "tool_builder_invalid_parameters",
            }));
        }

        let max_attempts = match optional_positive_usize(&arguments, "max_attempts") {
            Ok(value) => value.unwrap_or(self.policy.tool_builder_max_attempts),
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "error": error,
                    "reason_code": "tool_builder_invalid_max_attempts",
                }));
            }
        }
        .clamp(1, TOOL_BUILDER_MAX_ATTEMPTS_MAX);
        let timeout_ms = match optional_positive_u64(&arguments, "timeout_ms") {
            Ok(value) => value.unwrap_or(WASM_SANDBOX_TIMEOUT_MS_DEFAULT),
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "error": error,
                    "reason_code": "tool_builder_invalid_timeout_ms",
                }));
            }
        };
        let fuel_limit = match optional_positive_u64(&arguments, "fuel_limit") {
            Ok(value) => value.unwrap_or(WASM_SANDBOX_FUEL_LIMIT_DEFAULT),
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "error": error,
                    "reason_code": "tool_builder_invalid_fuel_limit",
                }));
            }
        };
        let memory_limit_bytes = match optional_positive_u64(&arguments, "memory_limit_bytes") {
            Ok(value) => value.unwrap_or(WASM_SANDBOX_MEMORY_LIMIT_BYTES_DEFAULT),
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "error": error,
                    "reason_code": "tool_builder_invalid_memory_limit_bytes",
                }));
            }
        };
        let max_response_bytes = match optional_positive_usize(&arguments, "max_response_bytes") {
            Ok(value) => value.unwrap_or(WASM_SANDBOX_MAX_RESPONSE_BYTES_DEFAULT),
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "error": error,
                    "reason_code": "tool_builder_invalid_max_response_bytes",
                }));
            }
        };
        let filesystem_mode = match optional_string(&arguments, "filesystem_mode") {
            Ok(Some(mode)) => match mode.as_str() {
                "deny" => WasmSandboxFilesystemMode::Deny,
                "read-only" => WasmSandboxFilesystemMode::ReadOnly,
                "read-write" => WasmSandboxFilesystemMode::ReadWrite,
                _ => {
                    return ToolExecutionResult::error(json!({
                        "error": "field 'filesystem_mode' must be one of: deny, read-only, read-write",
                        "reason_code": "tool_builder_invalid_filesystem_mode",
                    }));
                }
            },
            Ok(None) => WasmSandboxFilesystemMode::Deny,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "error": error,
                    "reason_code": "tool_builder_invalid_filesystem_mode",
                }));
            }
        };
        let network_mode = match optional_string(&arguments, "network_mode") {
            Ok(Some(mode)) => match mode.as_str() {
                "deny" => WasmSandboxNetworkMode::Deny,
                "allow" => WasmSandboxNetworkMode::Allow,
                _ => {
                    return ToolExecutionResult::error(json!({
                        "error": "field 'network_mode' must be one of: deny, allow",
                        "reason_code": "tool_builder_invalid_network_mode",
                    }));
                }
            },
            Ok(None) => WasmSandboxNetworkMode::Deny,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "error": error,
                    "reason_code": "tool_builder_invalid_network_mode",
                }));
            }
        };
        let env_allowlist = match optional_string_array_unbounded(&arguments, "env_allowlist") {
            Ok(values) => values,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "error": error,
                    "reason_code": "tool_builder_invalid_env_allowlist",
                }));
            }
        };
        let provided_wat_source = match optional_string(&arguments, "wat_source") {
            Ok(value) => value,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "error": error,
                    "reason_code": "tool_builder_invalid_wat_source",
                }));
            }
        };
        let output_root = match optional_string(&arguments, "output_root") {
            Ok(value) => resolve_builder_root_path(value, &self.policy.tool_builder_output_root),
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "error": error,
                    "reason_code": "tool_builder_invalid_output_root",
                }));
            }
        };
        let extension_root = match optional_string(&arguments, "extension_root") {
            Ok(value) => resolve_builder_root_path(value, &self.policy.tool_builder_extension_root),
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "error": error,
                    "reason_code": "tool_builder_invalid_extension_root",
                }));
            }
        };

        let request = GeneratedToolBuildRequest {
            tool_name: name,
            description,
            spec,
            parameters,
            output_root,
            extension_root,
            max_attempts,
            timeout_ms,
            wasm_limits: WasmSandboxLimits {
                fuel_limit,
                memory_limit_bytes,
                timeout_ms,
                max_response_bytes,
            },
            wasm_capabilities: WasmSandboxCapabilityProfile {
                filesystem_mode,
                network_mode,
                env_allowlist,
            },
            provided_wat_source,
        };
        match build_generated_wasm_tool(request) {
            Ok(report) => ToolExecutionResult::ok(json!({
                "schema_version": report.schema_version,
                "tool_name": report.tool_name,
                "manifest_id": report.manifest_id,
                "manifest_path": report.manifest_path.display().to_string(),
                "module_path": report.module_path.display().to_string(),
                "source_path": report.source_path.display().to_string(),
                "metadata_path": report.metadata_path.display().to_string(),
                "attempts": report.attempts,
                "reason_codes": report.reason_codes,
                "diagnostics": report.diagnostics,
            })),
            Err(error) => ToolExecutionResult::error(json!({
                "error": error.message,
                "reason_code": error.reason_code,
                "diagnostics": error.diagnostics,
            })),
        }
    }
}

fn resolve_builder_root_path(override_path: Option<String>, default_path: &Path) -> PathBuf {
    let configured = override_path
        .map(PathBuf::from)
        .unwrap_or_else(|| default_path.to_path_buf());
    if configured.is_absolute() {
        configured
    } else {
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(configured),
            Err(_) => configured,
        }
    }
}

/// Public struct `ReadTool` used across Tau components.
pub struct ReadTool {
    policy: Arc<ToolPolicy>,
}

impl ReadTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl AgentTool for ReadTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "read".to_string(),
            description: "Read a UTF-8 text file from disk".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to read" }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let path = match required_string(&arguments, "path") {
            Ok(path) => path,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };

        let resolved = match resolve_and_validate_path(&path, &self.policy, PathMode::Read) {
            Ok(path) => path,
            Err(error) => {
                return ToolExecutionResult::error(json!({ "path": path, "error": error }))
            }
        };
        if let Err(error) =
            validate_file_target(&resolved, PathMode::Read, self.policy.enforce_regular_files)
        {
            return ToolExecutionResult::error(json!({
                "path": resolved.display().to_string(),
                "error": error,
            }));
        }

        let metadata = match tokio::fs::metadata(&resolved).await {
            Ok(metadata) => metadata,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "path": resolved.display().to_string(),
                    "error": error.to_string(),
                }))
            }
        };

        if metadata.len() as usize > self.policy.max_file_read_bytes {
            return ToolExecutionResult::error(json!({
                "path": resolved.display().to_string(),
                "error": format!(
                    "file is too large ({} bytes), limit is {} bytes",
                    metadata.len(),
                    self.policy.max_file_read_bytes
                ),
            }));
        }

        match tokio::fs::read_to_string(&resolved).await {
            Ok(content) => ToolExecutionResult::ok(json!({
                "path": resolved.display().to_string(),
                "content": content,
            })),
            Err(error) => ToolExecutionResult::error(json!({
                "path": resolved.display().to_string(),
                "error": error.to_string(),
            })),
        }
    }
}

fn current_unix_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}

fn evaluate_protected_path_gate(
    policy: &ToolPolicy,
    tool_name: &str,
    path: &Path,
) -> Option<ToolExecutionResult> {
    if policy.allow_protected_path_mutations {
        return None;
    }

    let normalized_path = normalize_policy_path(path);
    let matched_protected_path = policy
        .protected_paths
        .iter()
        .find(|candidate| **candidate == normalized_path)?;
    let path_display = normalized_path.display().to_string();
    let matched_display = matched_protected_path.display().to_string();

    Some(ToolExecutionResult::error(json!({
        "policy_rule": "protected_path",
        "decision": "deny",
        "reason_code": "protected_path_denied",
        "action": format!("tool:{tool_name}"),
        "path": path_display,
        "protected_path": matched_display,
        "error": "path is protected by tool policy",
        "hint": "set TAU_ALLOW_PROTECTED_PATH_MUTATIONS=1 to allow protected path mutations for controlled maintenance windows",
    })))
}

/// Public struct `WriteTool` used across Tau components.
pub struct WriteTool {
    policy: Arc<ToolPolicy>,
}

impl WriteTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl AgentTool for WriteTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "write".to_string(),
            description: "Write UTF-8 text to disk, creating parent directories if needed"
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let path = match required_string(&arguments, "path") {
            Ok(path) => path,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };

        let content = match required_string(&arguments, "content") {
            Ok(content) => content,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };
        let content_size = content.len();
        if content_size > self.policy.max_file_write_bytes {
            return ToolExecutionResult::error(json!({
                "path": path,
                "error": format!(
                    "content is too large ({} bytes), limit is {} bytes",
                    content_size,
                    self.policy.max_file_write_bytes
                ),
            }));
        }

        let resolved = match resolve_and_validate_path(&path, &self.policy, PathMode::Write) {
            Ok(path) => path,
            Err(error) => {
                return ToolExecutionResult::error(json!({ "path": path, "error": error }))
            }
        };
        if let Err(error) = validate_file_target(
            &resolved,
            PathMode::Write,
            self.policy.enforce_regular_files,
        ) {
            return ToolExecutionResult::error(json!({
                "path": resolved.display().to_string(),
                "error": error,
            }));
        }
        if let Some(protected_path_result) =
            evaluate_protected_path_gate(&self.policy, "write", &resolved)
        {
            return protected_path_result;
        }

        if let Some(rbac_result) = evaluate_tool_rbac_gate(
            self.policy.rbac_principal.as_deref(),
            "write",
            self.policy.rbac_policy_path.as_deref(),
            json!({
                "path": resolved.display().to_string(),
                "content_bytes": content_size,
            }),
        ) {
            return rbac_result;
        }

        if let Some(approval_result) = evaluate_tool_approval_gate(ApprovalAction::ToolWrite {
            path: resolved.display().to_string(),
            content_bytes: content_size,
        }) {
            return approval_result;
        }

        if let Some(rate_limit_result) = evaluate_tool_rate_limit_gate(
            &self.policy,
            "write",
            json!({
                "path": resolved.display().to_string(),
                "content_bytes": content_size,
            }),
        ) {
            return rate_limit_result;
        }

        if let Some(parent) = resolved.parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(error) = tokio::fs::create_dir_all(parent).await {
                    return ToolExecutionResult::error(json!({
                        "path": resolved.display().to_string(),
                        "error": format!("failed to create parent directory: {error}"),
                    }));
                }
            }
        }

        match tokio::fs::write(&resolved, content.as_bytes()).await {
            Ok(()) => ToolExecutionResult::ok(json!({
                "path": resolved.display().to_string(),
                "bytes_written": content.len(),
            })),
            Err(error) => ToolExecutionResult::error(json!({
                "path": resolved.display().to_string(),
                "error": error.to_string(),
            })),
        }
    }
}

/// Public struct `EditTool` used across Tau components.
pub struct EditTool {
    policy: Arc<ToolPolicy>,
}

impl EditTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl AgentTool for EditTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "edit".to_string(),
            description: "Edit a file by replacing an existing string".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "find": { "type": "string" },
                    "replace": { "type": "string" },
                    "all": { "type": "boolean", "default": false }
                },
                "required": ["path", "find", "replace"],
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let path = match required_string(&arguments, "path") {
            Ok(path) => path,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };

        let find = match required_string(&arguments, "find") {
            Ok(find) => find,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };

        let replace = match required_string(&arguments, "replace") {
            Ok(replace) => replace,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };

        if find.is_empty() {
            return ToolExecutionResult::error(json!({
                "path": path,
                "error": "'find' must not be empty",
            }));
        }

        let resolved = match resolve_and_validate_path(&path, &self.policy, PathMode::Edit) {
            Ok(path) => path,
            Err(error) => {
                return ToolExecutionResult::error(json!({ "path": path, "error": error }))
            }
        };
        if let Err(error) =
            validate_file_target(&resolved, PathMode::Edit, self.policy.enforce_regular_files)
        {
            return ToolExecutionResult::error(json!({
                "path": resolved.display().to_string(),
                "error": error,
            }));
        }
        if let Some(protected_path_result) =
            evaluate_protected_path_gate(&self.policy, "edit", &resolved)
        {
            return protected_path_result;
        }

        if let Some(rbac_result) = evaluate_tool_rbac_gate(
            self.policy.rbac_principal.as_deref(),
            "edit",
            self.policy.rbac_policy_path.as_deref(),
            json!({
                "path": resolved.display().to_string(),
                "find": find,
                "replace_bytes": replace.len(),
            }),
        ) {
            return rbac_result;
        }

        if let Some(approval_result) = evaluate_tool_approval_gate(ApprovalAction::ToolEdit {
            path: resolved.display().to_string(),
            find: find.clone(),
            replace_bytes: replace.len(),
        }) {
            return approval_result;
        }

        if let Some(rate_limit_result) = evaluate_tool_rate_limit_gate(
            &self.policy,
            "edit",
            json!({
                "path": resolved.display().to_string(),
                "find": find.clone(),
                "replace_bytes": replace.len(),
            }),
        ) {
            return rate_limit_result;
        }

        let replace_all = arguments
            .get("all")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let source = match tokio::fs::read_to_string(&resolved).await {
            Ok(source) => source,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "path": resolved.display().to_string(),
                    "error": error.to_string(),
                }))
            }
        };

        let occurrences = source.matches(&find).count();
        if occurrences == 0 {
            return ToolExecutionResult::error(json!({
                "path": resolved.display().to_string(),
                "error": "target string not found",
            }));
        }

        let updated = if replace_all {
            source.replace(&find, &replace)
        } else {
            source.replacen(&find, &replace, 1)
        };
        if updated.len() > self.policy.max_file_write_bytes {
            return ToolExecutionResult::error(json!({
                "path": resolved.display().to_string(),
                "error": format!(
                    "edited content is too large ({} bytes), limit is {} bytes",
                    updated.len(),
                    self.policy.max_file_write_bytes
                ),
            }));
        }

        if let Err(error) = tokio::fs::write(&resolved, updated.as_bytes()).await {
            return ToolExecutionResult::error(json!({
                "path": resolved.display().to_string(),
                "error": error.to_string(),
            }));
        }

        let replacements = if replace_all { occurrences } else { 1 };
        ToolExecutionResult::ok(json!({
            "path": resolved.display().to_string(),
            "replacements": replacements,
        }))
    }
}

/// Public struct `BranchTool` used across Tau components.
pub struct BranchTool {
    policy: Arc<ToolPolicy>,
}

impl BranchTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl AgentTool for BranchTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "branch".to_string(),
            description: "Append a branch prompt to a session lineage with explicit parent control"
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to target session JSONL/SQLite file"
                    },
                    "prompt": {
                        "type": "string",
                        "description": format!(
                            "Branch prompt text (max {} characters)",
                            BRANCH_TOOL_MAX_PROMPT_CHARS
                        )
                    },
                    "parent_id": {
                        "type": "integer",
                        "description": "Optional parent entry id. Defaults to session head."
                    }
                },
                "required": ["path", "prompt"],
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let path = match required_string(&arguments, "path") {
            Ok(path) => path,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };
        let prompt = match required_string(&arguments, "prompt") {
            Ok(prompt) => prompt,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };
        let parent_id = match optional_u64(&arguments, "parent_id") {
            Ok(parent_id) => parent_id,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };
        if prompt.trim().is_empty() {
            return ToolExecutionResult::error(json!({
                "tool": "branch",
                "path": path,
                "reason_code": "branch_prompt_empty",
                "error": "prompt must not be empty",
            }));
        }
        if prompt.chars().count() > BRANCH_TOOL_MAX_PROMPT_CHARS {
            return ToolExecutionResult::error(json!({
                "tool": "branch",
                "path": path,
                "reason_code": "branch_prompt_too_large",
                "error": format!(
                    "prompt exceeds max length of {} characters",
                    BRANCH_TOOL_MAX_PROMPT_CHARS
                ),
            }));
        }

        let resolved = match resolve_and_validate_path(&path, &self.policy, PathMode::Write) {
            Ok(path) => path,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "path": path,
                    "error": error,
                }))
            }
        };
        if let Err(error) = validate_file_target(
            &resolved,
            PathMode::Write,
            self.policy.enforce_regular_files,
        ) {
            return ToolExecutionResult::error(json!({
                "path": resolved.display().to_string(),
                "error": error,
            }));
        }

        let mut store = match SessionStore::load(&resolved) {
            Ok(store) => store,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "tool": "branch",
                    "path": resolved.display().to_string(),
                    "reason_code": "session_branch_load_error",
                    "error": format!("failed to load session: {error}"),
                }))
            }
        };

        let before_entries = store.entries().len();
        let previous_head_id = store.head_id();
        let selected_parent_id = parent_id.or(previous_head_id);
        let branch_message = Message::user(prompt.clone());
        let branch_head_id = match store.append_messages(selected_parent_id, &[branch_message]) {
            Ok(Some(head)) => head,
            Ok(None) => {
                return ToolExecutionResult::error(json!({
                    "tool": "branch",
                    "path": resolved.display().to_string(),
                    "reason_code": "session_branch_append_noop",
                    "error": "branch append produced no new head",
                }))
            }
            Err(error) => {
                let error_string = error.to_string();
                let reason_code = if error_string.contains("parent id")
                    && error_string.contains("does not exist")
                {
                    "session_branch_parent_not_found"
                } else {
                    "session_branch_append_error"
                };
                return ToolExecutionResult::error(json!({
                    "tool": "branch",
                    "path": resolved.display().to_string(),
                    "reason_code": reason_code,
                    "parent_id": selected_parent_id,
                    "error": error_string,
                }));
            }
        };
        let after_entries = store.entries().len();

        ToolExecutionResult::ok(json!({
            "tool": "branch",
            "path": resolved.display().to_string(),
            "reason_code": "session_branch_created",
            "summary": "branch entry created",
            "selected_parent_id": selected_parent_id,
            "previous_head_id": previous_head_id,
            "branch_head_id": branch_head_id,
            "before_entries": before_entries,
            "after_entries": after_entries,
            "appended_entries": after_entries.saturating_sub(before_entries),
            "prompt_preview": session_message_preview(&Message::user(prompt)),
        }))
    }
}

/// Public struct `UndoTool` used across Tau components.
pub struct UndoTool {
    policy: Arc<ToolPolicy>,
}

impl UndoTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl AgentTool for UndoTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "undo".to_string(),
            description:
                "Move a session's active navigation head backward using persisted undo history"
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to target session JSONL file"
                    }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let path = match required_string(&arguments, "path") {
            Ok(path) => path,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };

        let resolved = match resolve_and_validate_path(&path, &self.policy, PathMode::Write) {
            Ok(path) => path,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "path": path,
                    "error": error,
                }))
            }
        };
        if let Err(error) = validate_file_target(
            &resolved,
            PathMode::Write,
            self.policy.enforce_regular_files,
        ) {
            return ToolExecutionResult::error(json!({
                "path": resolved.display().to_string(),
                "error": error,
            }));
        }

        let store = match SessionStore::load(&resolved) {
            Ok(store) => store,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "path": resolved.display().to_string(),
                    "reason_code": "session_navigation_load_error",
                    "error": format!("failed to load session: {error}"),
                }))
            }
        };

        let active_head = match resolve_session_navigation_head(&store) {
            Ok(active_head) => active_head,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "tool": "undo",
                    "path": resolved.display().to_string(),
                    "reason_code": "session_navigation_state_error",
                    "error": format!("failed to resolve navigation state: {error}"),
                }))
            }
        };
        let mut runtime = SessionRuntime { store, active_head };
        let transition = match undo_session_head(&mut runtime) {
            Ok(transition) => transition,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "tool": "undo",
                    "path": resolved.display().to_string(),
                    "reason_code": "session_navigation_state_error",
                    "error": format!("failed to execute undo: {error}"),
                }))
            }
        };

        if !transition.changed {
            return ToolExecutionResult::error(json!({
                "tool": "undo",
                "path": resolved.display().to_string(),
                "reason_code": "session_undo_empty_stack",
                "summary": "undo unavailable: no prior navigation target",
                "previous_head_id": transition.previous_head,
                "active_head_id": transition.active_head,
                "undo_depth": transition.undo_depth,
                "redo_depth": transition.redo_depth,
                "skipped_invalid_targets": transition.skipped_invalid_targets,
            }));
        }

        ToolExecutionResult::ok(json!({
            "tool": "undo",
            "path": resolved.display().to_string(),
            "reason_code": "session_undo_applied",
            "summary": "undo complete",
            "previous_head_id": transition.previous_head,
            "active_head_id": transition.active_head,
            "undo_depth": transition.undo_depth,
            "redo_depth": transition.redo_depth,
            "skipped_invalid_targets": transition.skipped_invalid_targets,
        }))
    }
}

/// Public struct `RedoTool` used across Tau components.
pub struct RedoTool {
    policy: Arc<ToolPolicy>,
}

impl RedoTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl AgentTool for RedoTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "redo".to_string(),
            description:
                "Move a session's active navigation head forward using persisted redo history"
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to target session JSONL file"
                    }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let path = match required_string(&arguments, "path") {
            Ok(path) => path,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };

        let resolved = match resolve_and_validate_path(&path, &self.policy, PathMode::Write) {
            Ok(path) => path,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "path": path,
                    "error": error,
                }))
            }
        };
        if let Err(error) = validate_file_target(
            &resolved,
            PathMode::Write,
            self.policy.enforce_regular_files,
        ) {
            return ToolExecutionResult::error(json!({
                "path": resolved.display().to_string(),
                "error": error,
            }));
        }

        let store = match SessionStore::load(&resolved) {
            Ok(store) => store,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "path": resolved.display().to_string(),
                    "reason_code": "session_navigation_load_error",
                    "error": format!("failed to load session: {error}"),
                }))
            }
        };

        let active_head = match resolve_session_navigation_head(&store) {
            Ok(active_head) => active_head,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "tool": "redo",
                    "path": resolved.display().to_string(),
                    "reason_code": "session_navigation_state_error",
                    "error": format!("failed to resolve navigation state: {error}"),
                }))
            }
        };
        let mut runtime = SessionRuntime { store, active_head };
        let transition = match redo_session_head(&mut runtime) {
            Ok(transition) => transition,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "tool": "redo",
                    "path": resolved.display().to_string(),
                    "reason_code": "session_navigation_state_error",
                    "error": format!("failed to execute redo: {error}"),
                }))
            }
        };

        if !transition.changed {
            return ToolExecutionResult::error(json!({
                "tool": "redo",
                "path": resolved.display().to_string(),
                "reason_code": "session_redo_empty_stack",
                "summary": "redo unavailable: no prior undone navigation target",
                "previous_head_id": transition.previous_head,
                "active_head_id": transition.active_head,
                "undo_depth": transition.undo_depth,
                "redo_depth": transition.redo_depth,
                "skipped_invalid_targets": transition.skipped_invalid_targets,
            }));
        }

        ToolExecutionResult::ok(json!({
            "tool": "redo",
            "path": resolved.display().to_string(),
            "reason_code": "session_redo_applied",
            "summary": "redo complete",
            "previous_head_id": transition.previous_head,
            "active_head_id": transition.active_head,
            "undo_depth": transition.undo_depth,
            "redo_depth": transition.redo_depth,
            "skipped_invalid_targets": transition.skipped_invalid_targets,
        }))
    }
}

/// Public struct `SkipTool` used across Tau components.
pub struct SkipTool {
    _policy: Arc<ToolPolicy>,
}

impl SkipTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { _policy: policy }
    }
}

#[async_trait]
impl AgentTool for SkipTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "skip".to_string(),
            description: "Suppress outbound user-facing response for the current turn".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "reason": {
                        "type": "string",
                        "description": "Optional audit/debug reason for suppressing the response"
                    }
                },
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let reason = match optional_string(&arguments, "reason") {
            Ok(reason) => reason.unwrap_or_default(),
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };
        ToolExecutionResult::ok(json!({
            "skip_response": true,
            "reason": reason,
            "reason_code": "skip_suppressed",
        }))
    }
}

/// Public struct `ReactTool` used across Tau components.
pub struct ReactTool {
    _policy: Arc<ToolPolicy>,
}

impl ReactTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { _policy: policy }
    }
}

#[async_trait]
impl AgentTool for ReactTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "react".to_string(),
            description: "Request emoji reaction delivery and suppress textual reply".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "emoji": {
                        "type": "string",
                        "description": "Emoji to dispatch to the target message"
                    },
                    "message_id": {
                        "type": "string",
                        "description": "Optional target message id; defaults to current event id in channel runtimes"
                    }
                },
                "required": ["emoji"],
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let emoji = match required_string(&arguments, "emoji") {
            Ok(value) => value.trim().to_string(),
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };
        if emoji.is_empty() {
            return ToolExecutionResult::error(json!({
                "error": "field 'emoji' must not be empty",
                "reason_code": "react_invalid_emoji",
            }));
        }
        let message_id = match optional_string(&arguments, "message_id") {
            Ok(value) => value
                .map(|raw| raw.trim().to_string())
                .filter(|value| !value.is_empty()),
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };
        ToolExecutionResult::ok(json!({
            "react_response": true,
            "emoji": emoji,
            "message_id": message_id,
            "reason_code": "react_requested",
            "suppress_response": true,
        }))
    }
}

/// Public struct `SendFileTool` used across Tau components.
pub struct SendFileTool {
    _policy: Arc<ToolPolicy>,
}

impl SendFileTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { _policy: policy }
    }
}

#[async_trait]
impl AgentTool for SendFileTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "send_file".to_string(),
            description: "Request file delivery and suppress textual reply".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Path or URL identifying the file to deliver"
                    },
                    "message": {
                        "type": "string",
                        "description": "Optional caption/message to include with the file"
                    }
                },
                "required": ["file_path"],
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let file_path = match required_string(&arguments, "file_path") {
            Ok(value) => value.trim().to_string(),
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };
        if file_path.is_empty() {
            return ToolExecutionResult::error(json!({
                "error": "field 'file_path' must not be empty",
                "reason_code": "send_file_invalid_path",
            }));
        }
        let message = match optional_string(&arguments, "message") {
            Ok(value) => value
                .map(|raw| raw.trim().to_string())
                .filter(|value| !value.is_empty()),
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };
        ToolExecutionResult::ok(json!({
            "send_file_response": true,
            "file_path": file_path,
            "message": message,
            "reason_code": "send_file_requested",
            "suppress_response": true,
        }))
    }
}

/// Public struct `HttpTool` used across Tau components.
pub struct HttpTool {
    policy: Arc<ToolPolicy>,
}

impl HttpTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }

    fn ssrf_guard(&self) -> SsrfGuard {
        SsrfGuard::new(SsrfProtectionConfig {
            enabled: true,
            allow_http: self.policy.http_allow_http,
            allow_private_network: self.policy.http_allow_private_network,
        })
    }

    fn ssrf_violation_result(
        &self,
        method: &Method,
        request_url: &str,
        endpoint: &str,
        violation: SsrfViolation,
    ) -> ToolExecutionResult {
        let retryable = violation.reason_code == "delivery_ssrf_dns_resolution_failed";
        ToolExecutionResult::error(json!({
            "policy_rule": "ssrf_guard",
            "tool": "http",
            "method": method.as_str(),
            "url": request_url,
            "final_url": endpoint,
            "reason_code": violation.reason_code,
            "retryable": retryable,
            "error": violation.detail,
        }))
    }
}

#[async_trait]
impl AgentTool for HttpTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "http".to_string(),
            description:
                "Send bounded outbound HTTP requests (GET/POST/PUT/DELETE) with SSRF guardrails"
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "Absolute target URL for the outbound request"
                    },
                    "method": {
                        "type": "string",
                        "description": "HTTP method: GET, POST, PUT, or DELETE (defaults to GET)"
                    },
                    "headers": {
                        "type": "object",
                        "description": "Optional string headers forwarded to the request",
                        "additionalProperties": { "type": "string" }
                    },
                    "json": {
                        "description": "Optional JSON payload for POST/PUT/DELETE requests"
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "description": "Optional per-request timeout (must be <= policy timeout cap)"
                    },
                    "max_response_bytes": {
                        "type": "integer",
                        "description": "Optional per-request response cap (must be <= policy response cap)"
                    }
                },
                "required": ["url"],
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let request_url = match required_string(&arguments, "url") {
            Ok(url) => url,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };
        let method = match parse_http_method(&arguments) {
            Ok(method) => method,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "tool": "http",
                    "url": request_url.as_str(),
                    "policy_rule": "http_method",
                    "reason_code": "http_invalid_method",
                    "error": error,
                }))
            }
        };
        let headers = match parse_http_headers(&arguments) {
            Ok(headers) => headers,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "tool": "http",
                    "method": method.as_str(),
                    "url": request_url.as_str(),
                    "policy_rule": "http_headers",
                    "reason_code": "http_invalid_headers",
                    "error": error,
                }))
            }
        };
        let header_names = headers
            .iter()
            .map(|(name, _value)| name.as_str().to_string())
            .collect::<Vec<_>>();
        let json_payload = arguments.get("json").cloned();
        if method == Method::GET && json_payload.is_some() {
            return ToolExecutionResult::error(json!({
                "tool": "http",
                "method": method.as_str(),
                "url": request_url.as_str(),
                "policy_rule": "http_method",
                "reason_code": "http_body_not_allowed",
                "error": "GET requests do not support a JSON payload",
            }));
        }

        let timeout_override_ms = match optional_positive_u64(&arguments, "timeout_ms") {
            Ok(value) => value,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };
        if let Some(timeout_override_ms) = timeout_override_ms {
            if timeout_override_ms > self.policy.http_timeout_ms {
                return ToolExecutionResult::error(json!({
                    "tool": "http",
                    "method": method.as_str(),
                    "url": request_url.as_str(),
                    "policy_rule": "http_timeout_ms",
                    "reason_code": "http_timeout_exceeds_policy",
                    "timeout_ms": timeout_override_ms,
                    "max_timeout_ms": self.policy.http_timeout_ms,
                    "error": format!(
                        "requested timeout {} ms exceeds policy cap {} ms",
                        timeout_override_ms,
                        self.policy.http_timeout_ms
                    ),
                }));
            }
        }
        let effective_timeout_ms = timeout_override_ms
            .unwrap_or(self.policy.http_timeout_ms)
            .max(1);

        let response_limit_override =
            match optional_positive_usize(&arguments, "max_response_bytes") {
                Ok(value) => value,
                Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
            };
        if let Some(limit) = response_limit_override {
            if limit > self.policy.http_max_response_bytes {
                return ToolExecutionResult::error(json!({
                    "tool": "http",
                    "method": method.as_str(),
                    "url": request_url.as_str(),
                    "policy_rule": "http_max_response_bytes",
                    "reason_code": "http_response_cap_exceeds_policy",
                    "max_response_bytes": limit,
                    "policy_max_response_bytes": self.policy.http_max_response_bytes,
                    "error": format!(
                        "requested response cap {} bytes exceeds policy cap {} bytes",
                        limit,
                        self.policy.http_max_response_bytes
                    ),
                }));
            }
        }
        let effective_response_limit = response_limit_override
            .unwrap_or(self.policy.http_max_response_bytes)
            .max(1);

        if let Some(rbac_result) = evaluate_tool_rbac_gate(
            self.policy.rbac_principal.as_deref(),
            "http",
            self.policy.rbac_policy_path.as_deref(),
            json!({
                "method": method.as_str(),
                "url": request_url.as_str(),
                "timeout_ms": effective_timeout_ms,
                "max_response_bytes": effective_response_limit,
                "headers": header_names.clone(),
                "has_json_payload": json_payload.is_some(),
            }),
        ) {
            return rbac_result;
        }

        if let Some(rate_limit_result) = evaluate_tool_rate_limit_gate(
            &self.policy,
            "http",
            json!({
                "method": method.as_str(),
                "url": request_url.as_str(),
                "timeout_ms": effective_timeout_ms,
                "max_response_bytes": effective_response_limit,
            }),
        ) {
            return rate_limit_result;
        }

        let client = match reqwest::Client::builder()
            .timeout(Duration::from_millis(effective_timeout_ms))
            .redirect(RedirectPolicy::none())
            .build()
        {
            Ok(client) => client,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "tool": "http",
                    "method": method.as_str(),
                    "url": request_url.as_str(),
                    "reason_code": "http_client_build_failed",
                    "error": format!("failed to initialize outbound HTTP client: {error}"),
                }))
            }
        };

        let ssrf_guard = self.ssrf_guard();
        let mut endpoint = match ssrf_guard.parse_and_validate_url(&request_url).await {
            Ok(url) => url,
            Err(violation) => {
                return self.ssrf_violation_result(
                    &method,
                    &request_url,
                    request_url.as_str(),
                    violation,
                )
            }
        };

        let started_at = Instant::now();
        let mut redirect_count = 0usize;
        loop {
            let mut request_builder = client.request(method.clone(), endpoint.clone());
            for (header_name, header_value) in &headers {
                request_builder = request_builder.header(header_name, header_value);
            }
            if let Some(payload) = &json_payload {
                request_builder = request_builder.json(payload);
            }

            let response = match request_builder.send().await {
                Ok(response) => response,
                Err(error) => {
                    let reason_code = if error.is_timeout() {
                        "http_request_timeout"
                    } else {
                        "http_transport_error"
                    };
                    let retryable = error.is_timeout() || error.is_connect();
                    return ToolExecutionResult::error(json!({
                        "tool": "http",
                        "method": method.as_str(),
                        "url": request_url.as_str(),
                        "final_url": endpoint.as_str(),
                        "reason_code": reason_code,
                        "retryable": retryable,
                        "error": error.to_string(),
                        "duration_ms": started_at.elapsed().as_millis(),
                    }));
                }
            };

            let status = response.status();
            if status.is_redirection() {
                if redirect_count >= self.policy.http_max_redirects {
                    return ToolExecutionResult::error(json!({
                        "tool": "http",
                        "method": method.as_str(),
                        "url": request_url.as_str(),
                        "final_url": endpoint.as_str(),
                        "policy_rule": "http_max_redirects",
                        "reason_code": "http_redirect_limit_exceeded",
                        "redirect_count": redirect_count,
                        "max_redirects": self.policy.http_max_redirects,
                        "http_status": status.as_u16(),
                        "error": format!(
                            "redirect count exceeded configured max_redirects={} for endpoint '{}'",
                            self.policy.http_max_redirects,
                            endpoint,
                        ),
                    }));
                }

                let location_header = match response.headers().get(reqwest::header::LOCATION) {
                    Some(location) => location,
                    None => {
                        return ToolExecutionResult::error(json!({
                            "tool": "http",
                            "method": method.as_str(),
                            "url": request_url.as_str(),
                            "final_url": endpoint.as_str(),
                            "reason_code": "http_redirect_missing_location",
                            "redirect_count": redirect_count,
                            "http_status": status.as_u16(),
                            "error": format!(
                                "received redirect status {} without a Location header",
                                status.as_u16()
                            ),
                        }))
                    }
                };

                let location = match location_header.to_str() {
                    Ok(value) => value,
                    Err(error) => {
                        return ToolExecutionResult::error(json!({
                            "tool": "http",
                            "method": method.as_str(),
                            "url": request_url.as_str(),
                            "final_url": endpoint.as_str(),
                            "reason_code": "http_redirect_invalid_location",
                            "redirect_count": redirect_count,
                            "http_status": status.as_u16(),
                            "error": format!("redirect Location header is not valid UTF-8: {error}"),
                        }))
                    }
                };

                let next_url = match endpoint.join(location) {
                    Ok(next_url) => next_url,
                    Err(error) => {
                        return ToolExecutionResult::error(json!({
                            "tool": "http",
                            "method": method.as_str(),
                            "url": request_url.as_str(),
                            "final_url": endpoint.as_str(),
                            "reason_code": "http_redirect_invalid_location",
                            "redirect_count": redirect_count,
                            "http_status": status.as_u16(),
                            "error": format!(
                                "redirect location '{}' could not be resolved against '{}': {error}",
                                location,
                                endpoint
                            ),
                        }))
                    }
                };

                if let Err(violation) = ssrf_guard.validate_url(&next_url).await {
                    return self.ssrf_violation_result(
                        &method,
                        &request_url,
                        next_url.as_str(),
                        violation,
                    );
                }

                endpoint = next_url;
                redirect_count = redirect_count.saturating_add(1);
                continue;
            }

            let response_headers = response.headers().clone();
            let mut response_bytes = Vec::new();
            let mut observed_bytes = 0usize;
            let mut response_stream = response;
            loop {
                let chunk = match response_stream.chunk().await {
                    Ok(chunk) => chunk,
                    Err(error) => {
                        return ToolExecutionResult::error(json!({
                            "tool": "http",
                            "method": method.as_str(),
                            "url": request_url.as_str(),
                            "final_url": endpoint.as_str(),
                            "reason_code": "http_response_read_error",
                            "retryable": true,
                            "error": error.to_string(),
                            "duration_ms": started_at.elapsed().as_millis(),
                        }));
                    }
                };
                let Some(chunk) = chunk else {
                    break;
                };
                observed_bytes = observed_bytes.saturating_add(chunk.len());
                if observed_bytes > effective_response_limit {
                    return ToolExecutionResult::error(json!({
                        "tool": "http",
                        "method": method.as_str(),
                        "url": request_url.as_str(),
                        "final_url": endpoint.as_str(),
                        "policy_rule": "http_max_response_bytes",
                        "reason_code": "http_response_too_large",
                        "response_bytes": observed_bytes,
                        "max_response_bytes": effective_response_limit,
                        "error": format!(
                            "response exceeded max_response_bytes cap of {} bytes",
                            effective_response_limit
                        ),
                    }));
                }
                response_bytes.extend_from_slice(&chunk);
            }

            let response_text = redact_secrets(&String::from_utf8_lossy(&response_bytes));
            let response_content_type = response_headers
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(ToString::to_string);
            let response_json = serde_json::from_slice::<Value>(&response_bytes).ok();
            let duration_ms = started_at.elapsed().as_millis() as u64;

            let mut payload = serde_json::Map::new();
            payload.insert("tool".to_string(), json!("http"));
            payload.insert("method".to_string(), json!(method.as_str()));
            payload.insert("url".to_string(), json!(request_url.as_str()));
            payload.insert("final_url".to_string(), json!(endpoint.as_str()));
            payload.insert("http_status".to_string(), json!(status.as_u16()));
            payload.insert("success".to_string(), json!(status.is_success()));
            payload.insert("redirect_count".to_string(), json!(redirect_count));
            payload.insert("duration_ms".to_string(), json!(duration_ms));
            payload.insert("timeout_ms".to_string(), json!(effective_timeout_ms));
            payload.insert(
                "max_response_bytes".to_string(),
                json!(effective_response_limit),
            );
            payload.insert("response_bytes".to_string(), json!(response_bytes.len()));
            payload.insert("request_header_names".to_string(), json!(header_names));
            payload.insert(
                "has_json_payload".to_string(),
                json!(json_payload.is_some()),
            );
            payload.insert("response_text".to_string(), json!(response_text));
            payload.insert(
                "ssrf_allow_http".to_string(),
                json!(self.policy.http_allow_http),
            );
            payload.insert(
                "ssrf_allow_private_network".to_string(),
                json!(self.policy.http_allow_private_network),
            );
            payload.insert(
                "max_redirects".to_string(),
                json!(self.policy.http_max_redirects),
            );
            if let Some(content_type) = response_content_type {
                payload.insert("content_type".to_string(), json!(content_type));
            } else {
                payload.insert("content_type".to_string(), Value::Null);
            }
            if let Some(response_json) = response_json {
                payload.insert("response_json".to_string(), response_json);
            }

            if status.is_success() {
                return ToolExecutionResult::ok(Value::Object(payload));
            }

            let (reason_code, retryable) = classify_http_status(status);
            payload.insert("reason_code".to_string(), json!(reason_code));
            payload.insert("retryable".to_string(), json!(retryable));
            payload.insert(
                "error".to_string(),
                json!(format!("request returned HTTP status {}", status.as_u16())),
            );
            return ToolExecutionResult::error(Value::Object(payload));
        }
    }
}

#[cfg(test)]
mod tests;
