//! Core tool registry wiring and policy-gated dispatch helpers.
//!
//! This module centralizes built-in tool registration, reserved-name handling,
//! and runtime metadata emitted for auditing/diagnostics when policy decisions
//! allow or deny tool execution.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `BashCommandProfile` values.
pub enum BashCommandProfile {
    Permissive,
    Balanced,
    Strict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `ToolPolicyPreset` values.
pub enum ToolPolicyPreset {
    Permissive,
    Balanced,
    Strict,
    Hardened,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `OsSandboxMode` values.
pub enum OsSandboxMode {
    Off,
    Auto,
    Force,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates how bash tool sandboxing is enforced when launchers are unavailable.
pub enum OsSandboxPolicyMode {
    BestEffort,
    Required,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported Docker network modes for sandbox execution.
pub enum OsSandboxDockerNetwork {
    None,
    Bridge,
    Host,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates how the tool rate limiter responds when a principal exceeds quota.
pub enum ToolRateLimitExceededBehavior {
    Reject,
    Defer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BashSandboxSpec {
    pub(super) program: String,
    pub(super) args: Vec<String>,
    pub(super) sandboxed: bool,
    pub(super) backend: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
/// Snapshot counters exposed for rate-limit observability.
pub struct ToolRateLimitCounters {
    pub throttle_events_total: u64,
    pub tracked_principals: usize,
}

#[derive(Debug, Default)]
struct ToolRateLimiterState {
    principals: HashMap<String, ToolRateLimiterPrincipalState>,
    throttle_events_total: u64,
}

#[derive(Debug, Clone)]
struct ToolRateLimiterPrincipalState {
    window_start_unix_ms: u64,
    requests_in_window: u32,
    throttle_events: u64,
}

impl ToolRateLimiterPrincipalState {
    fn new(now_unix_ms: u64) -> Self {
        Self {
            window_start_unix_ms: now_unix_ms,
            requests_in_window: 0,
            throttle_events: 0,
        }
    }
}

#[derive(Debug, Default)]
struct ToolRateLimiter {
    state: Mutex<ToolRateLimiterState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolRateLimitDecision {
    Allow,
    Throttled {
        retry_after_ms: u64,
        principal_throttle_events: u64,
        throttle_events_total: u64,
    },
}

impl ToolRateLimiter {
    fn evaluate(
        &self,
        principal: &str,
        max_requests: u32,
        window_ms: u64,
        now_unix_ms: u64,
    ) -> ToolRateLimitDecision {
        if max_requests == 0 || window_ms == 0 {
            return ToolRateLimitDecision::Allow;
        }

        let mut state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let entry = state
            .principals
            .entry(principal.to_string())
            .or_insert_with(|| ToolRateLimiterPrincipalState::new(now_unix_ms));

        if now_unix_ms.saturating_sub(entry.window_start_unix_ms) >= window_ms {
            entry.window_start_unix_ms = now_unix_ms;
            entry.requests_in_window = 0;
        }

        if entry.requests_in_window < max_requests {
            entry.requests_in_window = entry.requests_in_window.saturating_add(1);
            return ToolRateLimitDecision::Allow;
        }

        let (retry_after_ms, principal_throttle_events) = {
            entry.throttle_events = entry.throttle_events.saturating_add(1);
            let elapsed = now_unix_ms.saturating_sub(entry.window_start_unix_ms);
            let retry_after_ms = window_ms.saturating_sub(elapsed);
            (retry_after_ms, entry.throttle_events)
        };
        state.throttle_events_total = state.throttle_events_total.saturating_add(1);
        let throttle_events_total = state.throttle_events_total;
        ToolRateLimitDecision::Throttled {
            retry_after_ms,
            principal_throttle_events,
            throttle_events_total,
        }
    }

    fn counters(&self) -> ToolRateLimitCounters {
        let state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        ToolRateLimitCounters {
            throttle_events_total: state.throttle_events_total,
            tracked_principals: state.principals.len(),
        }
    }
}

#[derive(Debug, Clone)]
/// Public struct `ToolPolicy` used across Tau components.
pub struct ToolPolicy {
    pub allowed_roots: Vec<PathBuf>,
    pub protected_paths: Vec<PathBuf>,
    pub allow_protected_path_mutations: bool,
    pub policy_preset: ToolPolicyPreset,
    pub memory_state_dir: PathBuf,
    pub memory_search_default_limit: usize,
    pub memory_search_max_limit: usize,
    pub memory_embedding_dimensions: usize,
    pub memory_min_similarity: f32,
    pub memory_embedding_provider: Option<String>,
    pub memory_embedding_model: Option<String>,
    pub memory_embedding_api_base: Option<String>,
    pub memory_embedding_api_key: Option<String>,
    pub memory_embedding_timeout_ms: u64,
    pub memory_enable_hybrid_retrieval: bool,
    pub memory_bm25_k1: f32,
    pub memory_bm25_b: f32,
    pub memory_bm25_min_score: f32,
    pub memory_rrf_k: usize,
    pub memory_rrf_vector_weight: f32,
    pub memory_rrf_lexical_weight: f32,
    pub memory_enable_embedding_migration: bool,
    pub memory_benchmark_against_hash: bool,
    pub memory_benchmark_against_vector_only: bool,
    pub memory_write_max_summary_chars: usize,
    pub memory_write_max_facts: usize,
    pub memory_write_max_tags: usize,
    pub memory_write_max_fact_chars: usize,
    pub memory_write_max_tag_chars: usize,
    pub jobs_enabled: bool,
    pub jobs_state_dir: PathBuf,
    pub jobs_list_default_limit: usize,
    pub jobs_list_max_limit: usize,
    pub jobs_default_timeout_ms: u64,
    pub jobs_max_timeout_ms: u64,
    pub jobs_channel_store_root: PathBuf,
    pub jobs_default_session_path: Option<PathBuf>,
    pub max_file_read_bytes: usize,
    pub max_file_write_bytes: usize,
    pub max_command_output_bytes: usize,
    pub bash_timeout_ms: u64,
    pub max_command_length: usize,
    pub allow_command_newlines: bool,
    pub bash_profile: BashCommandProfile,
    pub allowed_commands: Vec<String>,
    pub os_sandbox_mode: OsSandboxMode,
    pub os_sandbox_policy_mode: OsSandboxPolicyMode,
    pub os_sandbox_command: Vec<String>,
    pub os_sandbox_docker_enabled: bool,
    pub os_sandbox_docker_image: String,
    pub os_sandbox_docker_network: OsSandboxDockerNetwork,
    pub os_sandbox_docker_memory_mb: u64,
    pub os_sandbox_docker_cpu_limit: f32,
    pub os_sandbox_docker_pids_limit: u64,
    pub os_sandbox_docker_read_only_rootfs: bool,
    pub os_sandbox_docker_env_allowlist: Vec<String>,
    pub http_timeout_ms: u64,
    pub http_max_response_bytes: usize,
    pub http_max_redirects: usize,
    pub http_allow_http: bool,
    pub http_allow_private_network: bool,
    pub enforce_regular_files: bool,
    pub bash_dry_run: bool,
    pub tool_policy_trace: bool,
    pub extension_policy_override_root: Option<PathBuf>,
    pub tool_builder_enabled: bool,
    pub tool_builder_output_root: PathBuf,
    pub tool_builder_extension_root: PathBuf,
    pub tool_builder_max_attempts: usize,
    pub rbac_principal: Option<String>,
    pub rbac_policy_path: Option<PathBuf>,
    pub tool_rate_limit_max_requests: u32,
    pub tool_rate_limit_window_ms: u64,
    pub tool_rate_limit_exceeded_behavior: ToolRateLimitExceededBehavior,
    rate_limiter: Arc<ToolRateLimiter>,
}

impl ToolPolicy {
    pub fn new(allowed_roots: Vec<PathBuf>) -> Self {
        let mut policy = Self {
            protected_paths: default_protected_paths(&allowed_roots),
            allow_protected_path_mutations: false,
            allowed_roots,
            policy_preset: ToolPolicyPreset::Balanced,
            memory_state_dir: PathBuf::from(".tau/memory"),
            memory_search_default_limit: MEMORY_SEARCH_DEFAULT_LIMIT,
            memory_search_max_limit: MEMORY_SEARCH_MAX_LIMIT,
            memory_embedding_dimensions: 128,
            memory_min_similarity: 0.55,
            memory_embedding_provider: None,
            memory_embedding_model: None,
            memory_embedding_api_base: None,
            memory_embedding_api_key: None,
            memory_embedding_timeout_ms: MEMORY_EMBEDDING_TIMEOUT_MS_DEFAULT,
            memory_enable_hybrid_retrieval: false,
            memory_bm25_k1: MEMORY_BM25_K1_DEFAULT,
            memory_bm25_b: MEMORY_BM25_B_DEFAULT,
            memory_bm25_min_score: MEMORY_BM25_MIN_SCORE_DEFAULT,
            memory_rrf_k: MEMORY_RRF_K_DEFAULT,
            memory_rrf_vector_weight: MEMORY_RRF_VECTOR_WEIGHT_DEFAULT,
            memory_rrf_lexical_weight: MEMORY_RRF_LEXICAL_WEIGHT_DEFAULT,
            memory_enable_embedding_migration: true,
            memory_benchmark_against_hash: false,
            memory_benchmark_against_vector_only: false,
            memory_write_max_summary_chars: MEMORY_WRITE_MAX_SUMMARY_CHARS,
            memory_write_max_facts: MEMORY_WRITE_MAX_FACTS,
            memory_write_max_tags: MEMORY_WRITE_MAX_TAGS,
            memory_write_max_fact_chars: MEMORY_WRITE_MAX_FACT_CHARS,
            memory_write_max_tag_chars: MEMORY_WRITE_MAX_TAG_CHARS,
            jobs_enabled: true,
            jobs_state_dir: PathBuf::from(".tau/jobs"),
            jobs_list_default_limit: JOBS_LIST_DEFAULT_LIMIT,
            jobs_list_max_limit: JOBS_LIST_MAX_LIMIT,
            jobs_default_timeout_ms: JOBS_DEFAULT_TIMEOUT_MS,
            jobs_max_timeout_ms: JOBS_MAX_TIMEOUT_MS,
            jobs_channel_store_root: PathBuf::from(".tau/channel-store"),
            jobs_default_session_path: Some(PathBuf::from(".tau/sessions/default.sqlite")),
            max_file_read_bytes: 1_000_000,
            max_file_write_bytes: 1_000_000,
            max_command_output_bytes: 16_000,
            bash_timeout_ms: 120_000,
            max_command_length: 4_096,
            allow_command_newlines: false,
            bash_profile: BashCommandProfile::Balanced,
            allowed_commands: BALANCED_COMMAND_ALLOWLIST
                .iter()
                .map(|command| (*command).to_string())
                .collect(),
            os_sandbox_mode: OsSandboxMode::Off,
            os_sandbox_policy_mode: OsSandboxPolicyMode::BestEffort,
            os_sandbox_command: Vec::new(),
            os_sandbox_docker_enabled: false,
            os_sandbox_docker_image: DOCKER_SANDBOX_DEFAULT_IMAGE.to_string(),
            os_sandbox_docker_network: OsSandboxDockerNetwork::None,
            os_sandbox_docker_memory_mb: DOCKER_SANDBOX_DEFAULT_MEMORY_MB,
            os_sandbox_docker_cpu_limit: DOCKER_SANDBOX_DEFAULT_CPUS,
            os_sandbox_docker_pids_limit: DOCKER_SANDBOX_DEFAULT_PIDS_LIMIT,
            os_sandbox_docker_read_only_rootfs: true,
            os_sandbox_docker_env_allowlist: Vec::new(),
            http_timeout_ms: TOOL_HTTP_TIMEOUT_MS_BALANCED,
            http_max_response_bytes: TOOL_HTTP_MAX_RESPONSE_BYTES_BALANCED,
            http_max_redirects: TOOL_HTTP_MAX_REDIRECTS_BALANCED,
            http_allow_http: false,
            http_allow_private_network: false,
            enforce_regular_files: true,
            bash_dry_run: false,
            tool_policy_trace: false,
            extension_policy_override_root: None,
            tool_builder_enabled: false,
            tool_builder_output_root: PathBuf::from(".tau/generated-tools"),
            tool_builder_extension_root: PathBuf::from(".tau/extensions/generated"),
            tool_builder_max_attempts: TOOL_BUILDER_MAX_ATTEMPTS_DEFAULT,
            rbac_principal: None,
            rbac_policy_path: None,
            tool_rate_limit_max_requests: TOOL_RATE_LIMIT_MAX_REQUESTS_BALANCED,
            tool_rate_limit_window_ms: TOOL_RATE_LIMIT_WINDOW_MS_DEFAULT,
            tool_rate_limit_exceeded_behavior: ToolRateLimitExceededBehavior::Reject,
            rate_limiter: Arc::new(ToolRateLimiter::default()),
        };
        policy.apply_preset(ToolPolicyPreset::Balanced);
        policy
    }

    pub fn add_protected_path(&mut self, path: PathBuf) {
        let normalized = normalize_policy_path(&path);
        if !self
            .protected_paths
            .iter()
            .any(|entry| entry == &normalized)
        {
            self.protected_paths.push(normalized);
            self.protected_paths.sort();
        }
    }

    pub fn set_bash_profile(&mut self, profile: BashCommandProfile) {
        self.bash_profile = profile;
        self.allowed_commands = match profile {
            BashCommandProfile::Permissive => Vec::new(),
            BashCommandProfile::Balanced => BALANCED_COMMAND_ALLOWLIST
                .iter()
                .map(|command| (*command).to_string())
                .collect(),
            BashCommandProfile::Strict => STRICT_COMMAND_ALLOWLIST
                .iter()
                .map(|command| (*command).to_string())
                .collect(),
        };
    }

    pub fn apply_preset(&mut self, preset: ToolPolicyPreset) {
        self.policy_preset = preset;
        match preset {
            ToolPolicyPreset::Permissive => {
                self.max_file_read_bytes = 2_000_000;
                self.max_file_write_bytes = 2_000_000;
                self.max_command_output_bytes = 32_000;
                self.bash_timeout_ms = 180_000;
                self.max_command_length = 8_192;
                self.allow_command_newlines = true;
                self.set_bash_profile(BashCommandProfile::Permissive);
                self.os_sandbox_mode = OsSandboxMode::Off;
                self.os_sandbox_policy_mode = OsSandboxPolicyMode::BestEffort;
                self.os_sandbox_command.clear();
                self.os_sandbox_docker_enabled = false;
                self.os_sandbox_docker_image = DOCKER_SANDBOX_DEFAULT_IMAGE.to_string();
                self.os_sandbox_docker_network = OsSandboxDockerNetwork::None;
                self.os_sandbox_docker_memory_mb = DOCKER_SANDBOX_DEFAULT_MEMORY_MB;
                self.os_sandbox_docker_cpu_limit = DOCKER_SANDBOX_DEFAULT_CPUS;
                self.os_sandbox_docker_pids_limit = DOCKER_SANDBOX_DEFAULT_PIDS_LIMIT;
                self.os_sandbox_docker_read_only_rootfs = true;
                self.os_sandbox_docker_env_allowlist.clear();
                self.http_timeout_ms = TOOL_HTTP_TIMEOUT_MS_PERMISSIVE;
                self.http_max_response_bytes = TOOL_HTTP_MAX_RESPONSE_BYTES_PERMISSIVE;
                self.http_max_redirects = TOOL_HTTP_MAX_REDIRECTS_PERMISSIVE;
                self.http_allow_http = false;
                self.http_allow_private_network = false;
                self.enforce_regular_files = false;
                self.tool_rate_limit_max_requests = TOOL_RATE_LIMIT_MAX_REQUESTS_PERMISSIVE;
                self.tool_rate_limit_window_ms = TOOL_RATE_LIMIT_WINDOW_MS_DEFAULT;
                self.tool_rate_limit_exceeded_behavior = ToolRateLimitExceededBehavior::Defer;
            }
            ToolPolicyPreset::Balanced => {
                self.max_file_read_bytes = 1_000_000;
                self.max_file_write_bytes = 1_000_000;
                self.max_command_output_bytes = 16_000;
                self.bash_timeout_ms = 120_000;
                self.max_command_length = 4_096;
                self.allow_command_newlines = false;
                self.set_bash_profile(BashCommandProfile::Balanced);
                self.os_sandbox_mode = OsSandboxMode::Off;
                self.os_sandbox_policy_mode = OsSandboxPolicyMode::BestEffort;
                self.os_sandbox_command.clear();
                self.os_sandbox_docker_enabled = false;
                self.os_sandbox_docker_image = DOCKER_SANDBOX_DEFAULT_IMAGE.to_string();
                self.os_sandbox_docker_network = OsSandboxDockerNetwork::None;
                self.os_sandbox_docker_memory_mb = DOCKER_SANDBOX_DEFAULT_MEMORY_MB;
                self.os_sandbox_docker_cpu_limit = DOCKER_SANDBOX_DEFAULT_CPUS;
                self.os_sandbox_docker_pids_limit = DOCKER_SANDBOX_DEFAULT_PIDS_LIMIT;
                self.os_sandbox_docker_read_only_rootfs = true;
                self.os_sandbox_docker_env_allowlist.clear();
                self.http_timeout_ms = TOOL_HTTP_TIMEOUT_MS_BALANCED;
                self.http_max_response_bytes = TOOL_HTTP_MAX_RESPONSE_BYTES_BALANCED;
                self.http_max_redirects = TOOL_HTTP_MAX_REDIRECTS_BALANCED;
                self.http_allow_http = false;
                self.http_allow_private_network = false;
                self.enforce_regular_files = true;
                self.tool_rate_limit_max_requests = TOOL_RATE_LIMIT_MAX_REQUESTS_BALANCED;
                self.tool_rate_limit_window_ms = TOOL_RATE_LIMIT_WINDOW_MS_DEFAULT;
                self.tool_rate_limit_exceeded_behavior = ToolRateLimitExceededBehavior::Reject;
            }
            ToolPolicyPreset::Strict => {
                self.max_file_read_bytes = 750_000;
                self.max_file_write_bytes = 750_000;
                self.max_command_output_bytes = 8_000;
                self.bash_timeout_ms = 90_000;
                self.max_command_length = 2_048;
                self.allow_command_newlines = false;
                self.set_bash_profile(BashCommandProfile::Strict);
                self.os_sandbox_mode = OsSandboxMode::Auto;
                self.os_sandbox_policy_mode = OsSandboxPolicyMode::Required;
                self.os_sandbox_command.clear();
                self.os_sandbox_docker_enabled = false;
                self.os_sandbox_docker_image = DOCKER_SANDBOX_DEFAULT_IMAGE.to_string();
                self.os_sandbox_docker_network = OsSandboxDockerNetwork::None;
                self.os_sandbox_docker_memory_mb = DOCKER_SANDBOX_DEFAULT_MEMORY_MB;
                self.os_sandbox_docker_cpu_limit = DOCKER_SANDBOX_DEFAULT_CPUS;
                self.os_sandbox_docker_pids_limit = DOCKER_SANDBOX_DEFAULT_PIDS_LIMIT;
                self.os_sandbox_docker_read_only_rootfs = true;
                self.os_sandbox_docker_env_allowlist.clear();
                self.http_timeout_ms = TOOL_HTTP_TIMEOUT_MS_STRICT;
                self.http_max_response_bytes = TOOL_HTTP_MAX_RESPONSE_BYTES_STRICT;
                self.http_max_redirects = TOOL_HTTP_MAX_REDIRECTS_STRICT;
                self.http_allow_http = false;
                self.http_allow_private_network = false;
                self.enforce_regular_files = true;
                self.tool_rate_limit_max_requests = TOOL_RATE_LIMIT_MAX_REQUESTS_STRICT;
                self.tool_rate_limit_window_ms = TOOL_RATE_LIMIT_WINDOW_MS_DEFAULT;
                self.tool_rate_limit_exceeded_behavior = ToolRateLimitExceededBehavior::Reject;
            }
            ToolPolicyPreset::Hardened => {
                self.max_file_read_bytes = 500_000;
                self.max_file_write_bytes = 500_000;
                self.max_command_output_bytes = 4_000;
                self.bash_timeout_ms = 60_000;
                self.max_command_length = 1_024;
                self.allow_command_newlines = false;
                self.set_bash_profile(BashCommandProfile::Strict);
                self.os_sandbox_mode = OsSandboxMode::Force;
                self.os_sandbox_policy_mode = OsSandboxPolicyMode::Required;
                self.os_sandbox_command.clear();
                self.os_sandbox_docker_enabled = false;
                self.os_sandbox_docker_image = DOCKER_SANDBOX_DEFAULT_IMAGE.to_string();
                self.os_sandbox_docker_network = OsSandboxDockerNetwork::None;
                self.os_sandbox_docker_memory_mb = DOCKER_SANDBOX_DEFAULT_MEMORY_MB;
                self.os_sandbox_docker_cpu_limit = DOCKER_SANDBOX_DEFAULT_CPUS;
                self.os_sandbox_docker_pids_limit = DOCKER_SANDBOX_DEFAULT_PIDS_LIMIT;
                self.os_sandbox_docker_read_only_rootfs = true;
                self.os_sandbox_docker_env_allowlist.clear();
                self.http_timeout_ms = TOOL_HTTP_TIMEOUT_MS_HARDENED;
                self.http_max_response_bytes = TOOL_HTTP_MAX_RESPONSE_BYTES_HARDENED;
                self.http_max_redirects = TOOL_HTTP_MAX_REDIRECTS_HARDENED;
                self.http_allow_http = false;
                self.http_allow_private_network = false;
                self.enforce_regular_files = true;
                self.tool_rate_limit_max_requests = TOOL_RATE_LIMIT_MAX_REQUESTS_HARDENED;
                self.tool_rate_limit_window_ms = TOOL_RATE_LIMIT_WINDOW_MS_DEFAULT;
                self.tool_rate_limit_exceeded_behavior = ToolRateLimitExceededBehavior::Reject;
            }
        }
    }

    pub fn rate_limit_counters(&self) -> ToolRateLimitCounters {
        self.rate_limiter.counters()
    }

    pub(crate) fn evaluate_rate_limit(
        &self,
        principal: &str,
        now_unix_ms: u64,
    ) -> Option<(u64, u64, u64)> {
        let decision = self.rate_limiter.evaluate(
            principal,
            self.tool_rate_limit_max_requests,
            self.tool_rate_limit_window_ms,
            now_unix_ms,
        );
        let ToolRateLimitDecision::Throttled {
            retry_after_ms,
            principal_throttle_events,
            throttle_events_total,
        } = decision
        else {
            return None;
        };
        Some((
            retry_after_ms,
            principal_throttle_events,
            throttle_events_total,
        ))
    }

    pub fn memory_embedding_provider_config(&self) -> Option<MemoryEmbeddingProviderConfig> {
        let provider = self.memory_embedding_provider.as_ref()?.trim();
        if provider.is_empty() {
            return None;
        }
        if provider.eq_ignore_ascii_case("local") {
            let model = self
                .memory_embedding_model
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("local-hash");
            return Some(MemoryEmbeddingProviderConfig {
                provider: "local".to_string(),
                model: model.to_string(),
                api_base: String::new(),
                api_key: String::new(),
                dimensions: self.memory_embedding_dimensions.max(1),
                timeout_ms: self.memory_embedding_timeout_ms.max(1),
            });
        }

        let model = self.memory_embedding_model.as_ref()?.trim();
        let api_base = self.memory_embedding_api_base.as_ref()?.trim();
        let api_key = self.memory_embedding_api_key.as_ref()?.trim();
        if model.is_empty() || api_base.is_empty() || api_key.is_empty() {
            return None;
        }

        Some(MemoryEmbeddingProviderConfig {
            provider: provider.to_string(),
            model: model.to_string(),
            api_base: api_base.to_string(),
            api_key: api_key.to_string(),
            dimensions: self.memory_embedding_dimensions.max(1),
            timeout_ms: self.memory_embedding_timeout_ms.max(1),
        })
    }
}

pub fn tool_policy_preset_name(preset: ToolPolicyPreset) -> &'static str {
    match preset {
        ToolPolicyPreset::Permissive => "permissive",
        ToolPolicyPreset::Balanced => "balanced",
        ToolPolicyPreset::Strict => "strict",
        ToolPolicyPreset::Hardened => "hardened",
    }
}

pub fn tool_rate_limit_behavior_name(behavior: ToolRateLimitExceededBehavior) -> &'static str {
    match behavior {
        ToolRateLimitExceededBehavior::Reject => "reject",
        ToolRateLimitExceededBehavior::Defer => "defer",
    }
}

pub fn register_builtin_tools(agent: &mut Agent, policy: ToolPolicy) {
    let policy = Arc::new(policy);
    agent.register_tool(ReadTool::new(policy.clone()));
    agent.register_tool(WriteTool::new(policy.clone()));
    agent.register_tool(EditTool::new(policy.clone()));
    agent.register_tool(MemoryWriteTool::new(policy.clone()));
    agent.register_tool(MemoryReadTool::new(policy.clone()));
    agent.register_tool(MemoryDeleteTool::new(policy.clone()));
    agent.register_tool(MemorySearchTool::new(policy.clone()));
    agent.register_tool(MemoryTreeTool::new(policy.clone()));
    agent.register_tool(SessionsListTool::new(policy.clone()));
    agent.register_tool(SessionsHistoryTool::new(policy.clone()));
    agent.register_tool(SessionsSearchTool::new(policy.clone()));
    agent.register_tool(SessionsStatsTool::new(policy.clone()));
    agent.register_tool(SessionsSendTool::new(policy.clone()));
    agent.register_tool(BranchTool::new(policy.clone()));
    agent.register_tool(JobsCreateTool::new(policy.clone()));
    agent.register_tool(JobsListTool::new(policy.clone()));
    agent.register_tool(JobsStatusTool::new(policy.clone()));
    agent.register_tool(JobsCancelTool::new(policy.clone()));
    agent.register_tool(UndoTool::new(policy.clone()));
    agent.register_tool(RedoTool::new(policy.clone()));
    agent.register_tool(SkipTool::new(policy.clone()));
    agent.register_tool(HttpTool::new(policy.clone()));
    if policy.tool_builder_enabled {
        agent.register_tool(ToolBuilderTool::new(policy.clone()));
    }
    agent.register_tool(BashTool::new(policy));
}

/// Returns the reserved registry of built-in agent tool names.
pub fn builtin_agent_tool_names() -> &'static [&'static str] {
    BUILTIN_AGENT_TOOL_NAMES
}

pub fn register_extension_tools(agent: &mut Agent, tools: &[ExtensionRegisteredTool]) {
    for tool in tools {
        agent.register_tool(ExtensionProcessTool::new(tool.clone()));
    }
}

struct ExtensionProcessTool {
    registration: ExtensionRegisteredTool,
}

impl ExtensionProcessTool {
    fn new(registration: ExtensionRegisteredTool) -> Self {
        Self { registration }
    }
}

#[async_trait]
impl AgentTool for ExtensionProcessTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.registration.name.clone(),
            description: self.registration.description.clone(),
            parameters: self.registration.parameters.clone(),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        match execute_extension_registered_tool(&self.registration, &arguments) {
            Ok(result) => ToolExecutionResult {
                content: result.content,
                is_error: result.is_error,
            },
            Err(error) => ToolExecutionResult::error(json!({
                "tool": self.registration.name,
                "extension_id": self.registration.extension_id,
                "extension_version": self.registration.extension_version,
                "manifest": self.registration.manifest_path.display().to_string(),
                "error": error.to_string(),
            })),
        }
    }
}
