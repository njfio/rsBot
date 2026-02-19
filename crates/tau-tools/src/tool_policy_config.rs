//! Composes runtime `ToolPolicy` from CLI/env inputs with fail-closed validation.
//!
//! This module is the policy resolution boundary between startup configuration
//! and execution-time tool enforcement. It documents preset mapping, sandbox
//! mode precedence, and guard-rail defaults used by downstream tool runtimes.

use anyhow::{anyhow, Context, Result};
use tau_cli::cli_args::Cli;
use tau_cli::cli_types::{
    CliBashProfile, CliOsSandboxDockerNetwork, CliOsSandboxMode, CliOsSandboxPolicyMode,
    CliToolPolicyPreset,
};

use crate::tools::{
    os_sandbox_docker_network_name, os_sandbox_policy_mode_name, tool_policy_preset_name,
    tool_rate_limit_behavior_name, BashCommandProfile, OsSandboxDockerNetwork, OsSandboxMode,
    OsSandboxPolicyMode, ToolPolicy,
};
use tau_memory::runtime::MemoryType;

const TOOL_POLICY_SCHEMA_VERSION: u32 = 13;
const PROTECTED_PATHS_ENV: &str = "TAU_PROTECTED_PATHS";
const ALLOW_PROTECTED_PATH_MUTATIONS_ENV: &str = "TAU_ALLOW_PROTECTED_PATH_MUTATIONS";
const MEMORY_EMBEDDING_PROVIDER_ENV: &str = "TAU_MEMORY_EMBEDDING_PROVIDER";
const MEMORY_EMBEDDING_MODEL_ENV: &str = "TAU_MEMORY_EMBEDDING_MODEL";
const MEMORY_EMBEDDING_API_BASE_ENV: &str = "TAU_MEMORY_EMBEDDING_API_BASE";
const MEMORY_EMBEDDING_API_KEY_ENV: &str = "TAU_MEMORY_EMBEDDING_API_KEY";
const MEMORY_EMBEDDING_TIMEOUT_MS_ENV: &str = "TAU_MEMORY_EMBEDDING_TIMEOUT_MS";
const MEMORY_ENABLE_HYBRID_RETRIEVAL_ENV: &str = "TAU_MEMORY_ENABLE_HYBRID_RETRIEVAL";
const MEMORY_BM25_K1_ENV: &str = "TAU_MEMORY_BM25_K1";
const MEMORY_BM25_B_ENV: &str = "TAU_MEMORY_BM25_B";
const MEMORY_BM25_MIN_SCORE_ENV: &str = "TAU_MEMORY_BM25_MIN_SCORE";
const MEMORY_RRF_K_ENV: &str = "TAU_MEMORY_RRF_K";
const MEMORY_RRF_VECTOR_WEIGHT_ENV: &str = "TAU_MEMORY_RRF_VECTOR_WEIGHT";
const MEMORY_RRF_LEXICAL_WEIGHT_ENV: &str = "TAU_MEMORY_RRF_LEXICAL_WEIGHT";
const MEMORY_ENABLE_EMBEDDING_MIGRATION_ENV: &str = "TAU_MEMORY_ENABLE_EMBEDDING_MIGRATION";
const MEMORY_BENCHMARK_AGAINST_HASH_ENV: &str = "TAU_MEMORY_BENCHMARK_AGAINST_HASH";
const MEMORY_BENCHMARK_AGAINST_VECTOR_ONLY_ENV: &str = "TAU_MEMORY_BENCHMARK_AGAINST_VECTOR_ONLY";
const MEMORY_DEFAULT_IMPORTANCE_IDENTITY_ENV: &str = "TAU_MEMORY_DEFAULT_IMPORTANCE_IDENTITY";
const MEMORY_DEFAULT_IMPORTANCE_GOAL_ENV: &str = "TAU_MEMORY_DEFAULT_IMPORTANCE_GOAL";
const MEMORY_DEFAULT_IMPORTANCE_DECISION_ENV: &str = "TAU_MEMORY_DEFAULT_IMPORTANCE_DECISION";
const MEMORY_DEFAULT_IMPORTANCE_TODO_ENV: &str = "TAU_MEMORY_DEFAULT_IMPORTANCE_TODO";
const MEMORY_DEFAULT_IMPORTANCE_PREFERENCE_ENV: &str = "TAU_MEMORY_DEFAULT_IMPORTANCE_PREFERENCE";
const MEMORY_DEFAULT_IMPORTANCE_FACT_ENV: &str = "TAU_MEMORY_DEFAULT_IMPORTANCE_FACT";
const MEMORY_DEFAULT_IMPORTANCE_EVENT_ENV: &str = "TAU_MEMORY_DEFAULT_IMPORTANCE_EVENT";
const MEMORY_DEFAULT_IMPORTANCE_OBSERVATION_ENV: &str = "TAU_MEMORY_DEFAULT_IMPORTANCE_OBSERVATION";

/// Build runtime tool policy from CLI arguments and environment overrides.
pub fn build_tool_policy(cli: &Cli) -> Result<ToolPolicy> {
    let cwd = std::env::current_dir().context("failed to resolve current directory")?;
    let mut roots = vec![cwd.clone()];
    roots.extend(cli.allow_path.clone());

    let mut policy = ToolPolicy::new(roots);
    policy.apply_preset(map_cli_tool_policy_preset(cli.tool_policy_preset));

    if cli.bash_timeout_ms != 120_000 {
        policy.bash_timeout_ms = cli.bash_timeout_ms.max(1);
    }
    if cli.max_tool_output_bytes != 16_000 {
        policy.max_command_output_bytes = cli.max_tool_output_bytes.max(128);
    }
    if cli.max_file_read_bytes != 1_000_000 {
        policy.max_file_read_bytes = cli.max_file_read_bytes.max(1_024);
    }
    if cli.max_file_write_bytes != 1_000_000 {
        policy.max_file_write_bytes = cli.max_file_write_bytes.max(1_024);
    }
    if cli.max_command_length != 4_096 {
        policy.max_command_length = cli.max_command_length.max(8);
    }
    if cli.allow_command_newlines {
        policy.allow_command_newlines = true;
    }
    if cli.bash_profile != CliBashProfile::Balanced {
        policy.set_bash_profile(map_cli_bash_profile(cli.bash_profile));
    }
    if cli.os_sandbox_mode != CliOsSandboxMode::Off {
        policy.os_sandbox_mode = map_cli_os_sandbox_mode(cli.os_sandbox_mode);
    }
    if let Some(policy_mode) = cli.os_sandbox_policy_mode {
        policy.os_sandbox_policy_mode = map_cli_os_sandbox_policy_mode(policy_mode);
    }
    if !cli.os_sandbox_command.is_empty() {
        policy.os_sandbox_command = parse_sandbox_command_tokens(&cli.os_sandbox_command)?;
    }
    policy.os_sandbox_docker_enabled = cli.os_sandbox_docker_enabled;
    policy.os_sandbox_docker_image = cli.os_sandbox_docker_image.trim().to_string();
    if policy.os_sandbox_docker_enabled && policy.os_sandbox_docker_image.is_empty() {
        return Err(anyhow!(
            "--os-sandbox-docker-image cannot be empty when --os-sandbox-docker-enabled is true"
        ));
    }
    policy.os_sandbox_docker_network =
        map_cli_os_sandbox_docker_network(cli.os_sandbox_docker_network);
    policy.os_sandbox_docker_memory_mb = cli.os_sandbox_docker_memory_mb.max(32);
    policy.os_sandbox_docker_cpu_limit = parse_docker_cpu_limit(&cli.os_sandbox_docker_cpus)?;
    policy.os_sandbox_docker_pids_limit = cli.os_sandbox_docker_pids_limit.max(16);
    policy.os_sandbox_docker_read_only_rootfs = cli.os_sandbox_docker_read_only_rootfs;
    policy.os_sandbox_docker_env_allowlist =
        parse_docker_env_allowlist(&cli.os_sandbox_docker_env)?;
    if cli.http_timeout_ms != 20_000 {
        policy.http_timeout_ms = cli.http_timeout_ms.max(1);
    }
    if cli.http_max_response_bytes != 256_000 {
        policy.http_max_response_bytes = cli.http_max_response_bytes.max(1);
    }
    if cli.http_max_redirects != 5 {
        policy.http_max_redirects = cli.http_max_redirects;
    }
    if cli.http_allow_http {
        policy.http_allow_http = true;
    }
    if cli.http_allow_private_network {
        policy.http_allow_private_network = true;
    }
    if !cli.enforce_regular_files {
        policy.enforce_regular_files = false;
    }
    if cli.bash_dry_run {
        policy.bash_dry_run = true;
    }
    if cli.tool_policy_trace {
        policy.tool_policy_trace = true;
    }
    if cli.extension_runtime_hooks {
        policy.extension_policy_override_root = Some(cli.extension_runtime_root.clone());
    }
    if cli.tool_builder_enabled {
        policy.tool_builder_enabled = true;
    }
    policy.tool_builder_output_root = if cli.tool_builder_output_root.is_absolute() {
        cli.tool_builder_output_root.clone()
    } else {
        cwd.join(cli.tool_builder_output_root.as_path())
    };
    policy.tool_builder_extension_root = if cli.tool_builder_extension_root.is_absolute() {
        cli.tool_builder_extension_root.clone()
    } else {
        cwd.join(cli.tool_builder_extension_root.as_path())
    };
    policy.tool_builder_max_attempts = cli.tool_builder_max_attempts.clamp(1, 8);
    if !cli.allow_command.is_empty() {
        for command in &cli.allow_command {
            let command = command.trim();
            if command.is_empty() {
                continue;
            }
            if !policy
                .allowed_commands
                .iter()
                .any(|existing| existing == command)
            {
                policy.allowed_commands.push(command.to_string());
            }
        }
    }
    policy.memory_state_dir = if cli.memory_state_dir.is_absolute() {
        cli.memory_state_dir.clone()
    } else {
        cwd.join(cli.memory_state_dir.as_path())
    };
    policy.jobs_enabled = cli.jobs_enabled;
    policy.jobs_state_dir = if cli.jobs_state_dir.is_absolute() {
        cli.jobs_state_dir.clone()
    } else {
        cwd.join(cli.jobs_state_dir.as_path())
    };
    policy.jobs_list_default_limit = cli.jobs_list_default_limit.max(1);
    policy.jobs_list_max_limit = cli.jobs_list_max_limit.max(policy.jobs_list_default_limit);
    policy.jobs_default_timeout_ms = cli.jobs_default_timeout_ms.max(1);
    policy.jobs_max_timeout_ms = cli.jobs_max_timeout_ms.max(policy.jobs_default_timeout_ms);
    policy.jobs_channel_store_root = if cli.channel_store_root.is_absolute() {
        cli.channel_store_root.clone()
    } else {
        cwd.join(cli.channel_store_root.as_path())
    };
    policy.jobs_default_session_path = if cli.no_session {
        None
    } else if cli.session.is_absolute() {
        Some(cli.session.clone())
    } else {
        Some(cwd.join(cli.session.as_path()))
    };
    if let Some(provider) = parse_optional_env_string(MEMORY_EMBEDDING_PROVIDER_ENV) {
        policy.memory_embedding_provider = Some(provider);
    }
    if let Some(model) = parse_optional_env_string(MEMORY_EMBEDDING_MODEL_ENV) {
        policy.memory_embedding_model = Some(model);
    }
    if let Some(api_base) = parse_optional_env_string(MEMORY_EMBEDDING_API_BASE_ENV) {
        policy.memory_embedding_api_base = Some(api_base);
    }
    if let Some(api_key) = parse_optional_env_string(MEMORY_EMBEDDING_API_KEY_ENV) {
        policy.memory_embedding_api_key = Some(api_key);
    }
    if let Some(timeout_ms) = parse_optional_env_u64(MEMORY_EMBEDDING_TIMEOUT_MS_ENV)? {
        policy.memory_embedding_timeout_ms = timeout_ms.max(1);
    }
    if let Some(enable_hybrid_retrieval) =
        parse_optional_env_bool(MEMORY_ENABLE_HYBRID_RETRIEVAL_ENV)?
    {
        policy.memory_enable_hybrid_retrieval = enable_hybrid_retrieval;
    }
    if let Some(bm25_k1) = parse_optional_env_f32(MEMORY_BM25_K1_ENV)? {
        policy.memory_bm25_k1 = bm25_k1.max(0.1);
    }
    if let Some(bm25_b) = parse_optional_env_f32(MEMORY_BM25_B_ENV)? {
        policy.memory_bm25_b = bm25_b.clamp(0.0, 1.0);
    }
    if let Some(bm25_min_score) = parse_optional_env_f32(MEMORY_BM25_MIN_SCORE_ENV)? {
        policy.memory_bm25_min_score = bm25_min_score.max(0.0);
    }
    if let Some(rrf_k) = parse_optional_env_u64(MEMORY_RRF_K_ENV)? {
        policy.memory_rrf_k = (rrf_k as usize).max(1);
    }
    if let Some(rrf_vector_weight) = parse_optional_env_f32(MEMORY_RRF_VECTOR_WEIGHT_ENV)? {
        policy.memory_rrf_vector_weight = rrf_vector_weight.max(0.0);
    }
    if let Some(rrf_lexical_weight) = parse_optional_env_f32(MEMORY_RRF_LEXICAL_WEIGHT_ENV)? {
        policy.memory_rrf_lexical_weight = rrf_lexical_weight.max(0.0);
    }
    if let Some(enable_migration) = parse_optional_env_bool(MEMORY_ENABLE_EMBEDDING_MIGRATION_ENV)?
    {
        policy.memory_enable_embedding_migration = enable_migration;
    }
    if let Some(benchmark_against_hash) =
        parse_optional_env_bool(MEMORY_BENCHMARK_AGAINST_HASH_ENV)?
    {
        policy.memory_benchmark_against_hash = benchmark_against_hash;
    }
    if let Some(benchmark_against_vector_only) =
        parse_optional_env_bool(MEMORY_BENCHMARK_AGAINST_VECTOR_ONLY_ENV)?
    {
        policy.memory_benchmark_against_vector_only = benchmark_against_vector_only;
    }
    apply_memory_type_importance_override(
        &mut policy.memory_default_importance_profile,
        MemoryType::Identity,
        MEMORY_DEFAULT_IMPORTANCE_IDENTITY_ENV,
    )?;
    apply_memory_type_importance_override(
        &mut policy.memory_default_importance_profile,
        MemoryType::Goal,
        MEMORY_DEFAULT_IMPORTANCE_GOAL_ENV,
    )?;
    apply_memory_type_importance_override(
        &mut policy.memory_default_importance_profile,
        MemoryType::Decision,
        MEMORY_DEFAULT_IMPORTANCE_DECISION_ENV,
    )?;
    apply_memory_type_importance_override(
        &mut policy.memory_default_importance_profile,
        MemoryType::Todo,
        MEMORY_DEFAULT_IMPORTANCE_TODO_ENV,
    )?;
    apply_memory_type_importance_override(
        &mut policy.memory_default_importance_profile,
        MemoryType::Preference,
        MEMORY_DEFAULT_IMPORTANCE_PREFERENCE_ENV,
    )?;
    apply_memory_type_importance_override(
        &mut policy.memory_default_importance_profile,
        MemoryType::Fact,
        MEMORY_DEFAULT_IMPORTANCE_FACT_ENV,
    )?;
    apply_memory_type_importance_override(
        &mut policy.memory_default_importance_profile,
        MemoryType::Event,
        MEMORY_DEFAULT_IMPORTANCE_EVENT_ENV,
    )?;
    apply_memory_type_importance_override(
        &mut policy.memory_default_importance_profile,
        MemoryType::Observation,
        MEMORY_DEFAULT_IMPORTANCE_OBSERVATION_ENV,
    )?;
    policy.memory_default_importance_profile.validate()?;

    let memory_embedding_provider_is_remote = policy
        .memory_embedding_provider
        .as_deref()
        .map(str::trim)
        .filter(|provider| !provider.is_empty())
        .map(|provider| !provider.eq_ignore_ascii_case("local"))
        .unwrap_or(false);
    if policy.memory_embedding_api_base.is_none() && memory_embedding_provider_is_remote {
        policy.memory_embedding_api_base = Some(cli.api_base.clone());
    }
    if policy.memory_embedding_api_key.is_none() && memory_embedding_provider_is_remote {
        policy.memory_embedding_api_key = cli.openai_api_key.clone().or(cli.api_key.clone());
    }

    if let Some(allow_mutations) = parse_optional_env_bool(ALLOW_PROTECTED_PATH_MUTATIONS_ENV)? {
        policy.allow_protected_path_mutations = allow_mutations;
    }

    for protected_path in parse_protected_paths_env(PROTECTED_PATHS_ENV)? {
        policy.add_protected_path(protected_path);
    }

    Ok(policy)
}

/// Parse --os-sandbox-command values into normalized command tokens.
pub fn parse_sandbox_command_tokens(raw_tokens: &[String]) -> Result<Vec<String>> {
    let mut parsed = Vec::new();
    for raw in raw_tokens {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let tokens = shell_words::split(trimmed).map_err(|error| {
            anyhow!("invalid --os-sandbox-command token '{}': {error}", trimmed)
        })?;
        if tokens.is_empty() {
            continue;
        }
        parsed.extend(tokens);
    }
    Ok(parsed)
}

/// Convert tool policy into JSON payload for diagnostics and audit output.
pub fn tool_policy_to_json(policy: &ToolPolicy) -> serde_json::Value {
    let rate_limit_counters = policy.rate_limit_counters();
    let mut payload = serde_json::Map::new();
    payload.insert(
        "schema_version".to_string(),
        serde_json::json!(TOOL_POLICY_SCHEMA_VERSION),
    );
    payload.insert(
        "preset".to_string(),
        serde_json::json!(tool_policy_preset_name(policy.policy_preset)),
    );
    payload.insert(
        "allowed_roots".to_string(),
        serde_json::json!(policy
            .allowed_roots
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()),
    );
    payload.insert(
        "protected_paths".to_string(),
        serde_json::json!(policy
            .protected_paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()),
    );
    payload.insert(
        "allow_protected_path_mutations".to_string(),
        serde_json::json!(policy.allow_protected_path_mutations),
    );
    payload.insert(
        "memory_state_dir".to_string(),
        serde_json::json!(policy.memory_state_dir.display().to_string()),
    );
    payload.insert(
        "memory_search_default_limit".to_string(),
        serde_json::json!(policy.memory_search_default_limit),
    );
    payload.insert(
        "memory_search_max_limit".to_string(),
        serde_json::json!(policy.memory_search_max_limit),
    );
    payload.insert(
        "memory_embedding_dimensions".to_string(),
        serde_json::json!(policy.memory_embedding_dimensions),
    );
    payload.insert(
        "memory_min_similarity".to_string(),
        serde_json::json!(policy.memory_min_similarity),
    );
    payload.insert(
        "memory_default_importance_profile".to_string(),
        serde_json::json!({
            "identity": policy.memory_default_importance_profile.identity,
            "goal": policy.memory_default_importance_profile.goal,
            "decision": policy.memory_default_importance_profile.decision,
            "todo": policy.memory_default_importance_profile.todo,
            "preference": policy.memory_default_importance_profile.preference,
            "fact": policy.memory_default_importance_profile.fact,
            "event": policy.memory_default_importance_profile.event,
            "observation": policy.memory_default_importance_profile.observation,
        }),
    );
    payload.insert(
        "memory_embedding_provider".to_string(),
        serde_json::json!(policy.memory_embedding_provider.clone()),
    );
    payload.insert(
        "memory_embedding_model".to_string(),
        serde_json::json!(policy.memory_embedding_model.clone()),
    );
    payload.insert(
        "memory_embedding_api_base".to_string(),
        serde_json::json!(policy.memory_embedding_api_base.clone()),
    );
    payload.insert(
        "memory_embedding_api_key_present".to_string(),
        serde_json::json!(policy
            .memory_embedding_api_key
            .as_ref()
            .is_some_and(|value| !value.trim().is_empty())),
    );
    payload.insert(
        "memory_embedding_timeout_ms".to_string(),
        serde_json::json!(policy.memory_embedding_timeout_ms),
    );
    payload.insert(
        "memory_enable_hybrid_retrieval".to_string(),
        serde_json::json!(policy.memory_enable_hybrid_retrieval),
    );
    payload.insert(
        "memory_bm25_k1".to_string(),
        serde_json::json!(policy.memory_bm25_k1),
    );
    payload.insert(
        "memory_bm25_b".to_string(),
        serde_json::json!(policy.memory_bm25_b),
    );
    payload.insert(
        "memory_bm25_min_score".to_string(),
        serde_json::json!(policy.memory_bm25_min_score),
    );
    payload.insert(
        "memory_rrf_k".to_string(),
        serde_json::json!(policy.memory_rrf_k),
    );
    payload.insert(
        "memory_rrf_vector_weight".to_string(),
        serde_json::json!(policy.memory_rrf_vector_weight),
    );
    payload.insert(
        "memory_rrf_lexical_weight".to_string(),
        serde_json::json!(policy.memory_rrf_lexical_weight),
    );
    payload.insert(
        "memory_enable_embedding_migration".to_string(),
        serde_json::json!(policy.memory_enable_embedding_migration),
    );
    payload.insert(
        "memory_benchmark_against_hash".to_string(),
        serde_json::json!(policy.memory_benchmark_against_hash),
    );
    payload.insert(
        "memory_benchmark_against_vector_only".to_string(),
        serde_json::json!(policy.memory_benchmark_against_vector_only),
    );
    payload.insert(
        "memory_write_max_summary_chars".to_string(),
        serde_json::json!(policy.memory_write_max_summary_chars),
    );
    payload.insert(
        "memory_write_max_facts".to_string(),
        serde_json::json!(policy.memory_write_max_facts),
    );
    payload.insert(
        "memory_write_max_tags".to_string(),
        serde_json::json!(policy.memory_write_max_tags),
    );
    payload.insert(
        "memory_write_max_fact_chars".to_string(),
        serde_json::json!(policy.memory_write_max_fact_chars),
    );
    payload.insert(
        "memory_write_max_tag_chars".to_string(),
        serde_json::json!(policy.memory_write_max_tag_chars),
    );
    payload.insert(
        "jobs_enabled".to_string(),
        serde_json::json!(policy.jobs_enabled),
    );
    payload.insert(
        "jobs_state_dir".to_string(),
        serde_json::json!(policy.jobs_state_dir.display().to_string()),
    );
    payload.insert(
        "jobs_list_default_limit".to_string(),
        serde_json::json!(policy.jobs_list_default_limit),
    );
    payload.insert(
        "jobs_list_max_limit".to_string(),
        serde_json::json!(policy.jobs_list_max_limit),
    );
    payload.insert(
        "jobs_default_timeout_ms".to_string(),
        serde_json::json!(policy.jobs_default_timeout_ms),
    );
    payload.insert(
        "jobs_max_timeout_ms".to_string(),
        serde_json::json!(policy.jobs_max_timeout_ms),
    );
    payload.insert(
        "jobs_channel_store_root".to_string(),
        serde_json::json!(policy.jobs_channel_store_root.display().to_string()),
    );
    payload.insert(
        "jobs_default_session_path".to_string(),
        serde_json::json!(policy
            .jobs_default_session_path
            .as_ref()
            .map(|path| path.display().to_string())),
    );
    payload.insert(
        "max_file_read_bytes".to_string(),
        serde_json::json!(policy.max_file_read_bytes),
    );
    payload.insert(
        "max_file_write_bytes".to_string(),
        serde_json::json!(policy.max_file_write_bytes),
    );
    payload.insert(
        "max_command_output_bytes".to_string(),
        serde_json::json!(policy.max_command_output_bytes),
    );
    payload.insert(
        "bash_timeout_ms".to_string(),
        serde_json::json!(policy.bash_timeout_ms),
    );
    payload.insert(
        "max_command_length".to_string(),
        serde_json::json!(policy.max_command_length),
    );
    payload.insert(
        "allow_command_newlines".to_string(),
        serde_json::json!(policy.allow_command_newlines),
    );
    payload.insert(
        "bash_profile".to_string(),
        serde_json::json!(format!("{:?}", policy.bash_profile).to_lowercase()),
    );
    payload.insert(
        "allowed_commands".to_string(),
        serde_json::json!(policy.allowed_commands.clone()),
    );
    payload.insert(
        "os_sandbox_mode".to_string(),
        serde_json::json!(format!("{:?}", policy.os_sandbox_mode).to_lowercase()),
    );
    payload.insert(
        "os_sandbox_policy_mode".to_string(),
        serde_json::json!(os_sandbox_policy_mode_name(policy.os_sandbox_policy_mode)),
    );
    payload.insert(
        "os_sandbox_command".to_string(),
        serde_json::json!(policy.os_sandbox_command.clone()),
    );
    payload.insert(
        "os_sandbox_docker_enabled".to_string(),
        serde_json::json!(policy.os_sandbox_docker_enabled),
    );
    payload.insert(
        "os_sandbox_docker_image".to_string(),
        serde_json::json!(policy.os_sandbox_docker_image.clone()),
    );
    payload.insert(
        "os_sandbox_docker_network".to_string(),
        serde_json::json!(os_sandbox_docker_network_name(
            policy.os_sandbox_docker_network
        )),
    );
    payload.insert(
        "os_sandbox_docker_memory_mb".to_string(),
        serde_json::json!(policy.os_sandbox_docker_memory_mb),
    );
    payload.insert(
        "os_sandbox_docker_cpu_limit".to_string(),
        serde_json::json!(policy.os_sandbox_docker_cpu_limit),
    );
    payload.insert(
        "os_sandbox_docker_pids_limit".to_string(),
        serde_json::json!(policy.os_sandbox_docker_pids_limit),
    );
    payload.insert(
        "os_sandbox_docker_read_only_rootfs".to_string(),
        serde_json::json!(policy.os_sandbox_docker_read_only_rootfs),
    );
    payload.insert(
        "os_sandbox_docker_env_allowlist".to_string(),
        serde_json::json!(policy.os_sandbox_docker_env_allowlist.clone()),
    );
    payload.insert(
        "http_timeout_ms".to_string(),
        serde_json::json!(policy.http_timeout_ms),
    );
    payload.insert(
        "http_max_response_bytes".to_string(),
        serde_json::json!(policy.http_max_response_bytes),
    );
    payload.insert(
        "http_max_redirects".to_string(),
        serde_json::json!(policy.http_max_redirects),
    );
    payload.insert(
        "http_allow_http".to_string(),
        serde_json::json!(policy.http_allow_http),
    );
    payload.insert(
        "http_allow_private_network".to_string(),
        serde_json::json!(policy.http_allow_private_network),
    );
    payload.insert(
        "enforce_regular_files".to_string(),
        serde_json::json!(policy.enforce_regular_files),
    );
    payload.insert(
        "bash_dry_run".to_string(),
        serde_json::json!(policy.bash_dry_run),
    );
    payload.insert(
        "tool_policy_trace".to_string(),
        serde_json::json!(policy.tool_policy_trace),
    );
    payload.insert(
        "extension_policy_override_root".to_string(),
        serde_json::json!(policy
            .extension_policy_override_root
            .as_ref()
            .map(|path| path.display().to_string())),
    );
    payload.insert(
        "tool_builder_enabled".to_string(),
        serde_json::json!(policy.tool_builder_enabled),
    );
    payload.insert(
        "tool_builder_output_root".to_string(),
        serde_json::json!(policy.tool_builder_output_root.display().to_string()),
    );
    payload.insert(
        "tool_builder_extension_root".to_string(),
        serde_json::json!(policy.tool_builder_extension_root.display().to_string()),
    );
    payload.insert(
        "tool_builder_max_attempts".to_string(),
        serde_json::json!(policy.tool_builder_max_attempts),
    );
    payload.insert(
        "rbac_principal".to_string(),
        serde_json::json!(policy.rbac_principal.clone()),
    );
    payload.insert(
        "rbac_policy_path".to_string(),
        serde_json::json!(policy
            .rbac_policy_path
            .as_ref()
            .map(|path| path.display().to_string())),
    );
    payload.insert(
        "tool_rate_limit".to_string(),
        serde_json::json!({
            "max_requests": policy.tool_rate_limit_max_requests,
            "window_ms": policy.tool_rate_limit_window_ms,
            "exceeded_behavior": tool_rate_limit_behavior_name(policy.tool_rate_limit_exceeded_behavior),
            "throttle_events_total": rate_limit_counters.throttle_events_total,
            "tracked_principals": rate_limit_counters.tracked_principals,
        }),
    );

    serde_json::Value::Object(payload)
}

fn parse_optional_env_bool(name: &str) -> Result<Option<bool>> {
    let Some(raw) = std::env::var_os(name) else {
        return Ok(None);
    };
    let raw = raw.to_string_lossy();
    let value = raw.trim().to_ascii_lowercase();
    if value.is_empty() {
        return Ok(None);
    }
    match value.as_str() {
        "1" | "true" | "yes" | "on" => Ok(Some(true)),
        "0" | "false" | "no" | "off" => Ok(Some(false)),
        _ => Err(anyhow!(
            "invalid {} value '{}': expected one of 1,true,yes,on,0,false,no,off",
            name,
            raw.trim()
        )),
    }
}

fn parse_optional_env_string(name: &str) -> Option<String> {
    let raw = std::env::var_os(name)?;
    let value = raw.to_string_lossy();
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_optional_env_u64(name: &str) -> Result<Option<u64>> {
    let Some(raw) = std::env::var_os(name) else {
        return Ok(None);
    };
    let value = raw.to_string_lossy();
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let parsed = trimmed.parse::<u64>().map_err(|error| {
        anyhow!(
            "invalid {} value '{}': expected unsigned integer ({error})",
            name,
            trimmed
        )
    })?;
    Ok(Some(parsed))
}

fn parse_optional_env_f32(name: &str) -> Result<Option<f32>> {
    let Some(raw) = std::env::var_os(name) else {
        return Ok(None);
    };
    let value = raw.to_string_lossy();
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let parsed = trimmed.parse::<f32>().map_err(|error| {
        anyhow!(
            "invalid {} value '{}': expected float ({error})",
            name,
            trimmed
        )
    })?;
    Ok(Some(parsed))
}

fn apply_memory_type_importance_override(
    profile: &mut tau_memory::runtime::MemoryTypeImportanceProfile,
    memory_type: MemoryType,
    env_name: &str,
) -> Result<()> {
    let Some(value) = parse_optional_env_f32(env_name)? else {
        return Ok(());
    };
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(anyhow!(
            "{env_name} must be finite and within 0.0..=1.0 (received {})",
            value
        ));
    }
    profile.set_importance(memory_type, value);
    Ok(())
}

fn parse_protected_paths_env(name: &str) -> Result<Vec<std::path::PathBuf>> {
    let Some(raw) = std::env::var_os(name) else {
        return Ok(Vec::new());
    };
    let cwd = std::env::current_dir().context("failed to resolve current directory")?;
    let mut paths = Vec::new();
    for token in raw.to_string_lossy().split(',') {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parsed = std::path::PathBuf::from(trimmed);
        let absolute = if parsed.is_absolute() {
            parsed
        } else {
            cwd.join(parsed)
        };
        if !paths.iter().any(|existing| existing == &absolute) {
            paths.push(absolute);
        }
    }
    Ok(paths)
}

fn map_cli_bash_profile(value: CliBashProfile) -> BashCommandProfile {
    match value {
        CliBashProfile::Permissive => BashCommandProfile::Permissive,
        CliBashProfile::Balanced => BashCommandProfile::Balanced,
        CliBashProfile::Strict => BashCommandProfile::Strict,
    }
}

fn map_cli_os_sandbox_mode(value: CliOsSandboxMode) -> OsSandboxMode {
    match value {
        CliOsSandboxMode::Off => OsSandboxMode::Off,
        CliOsSandboxMode::Auto => OsSandboxMode::Auto,
        CliOsSandboxMode::Force => OsSandboxMode::Force,
    }
}

fn map_cli_os_sandbox_policy_mode(value: CliOsSandboxPolicyMode) -> OsSandboxPolicyMode {
    match value {
        CliOsSandboxPolicyMode::BestEffort => OsSandboxPolicyMode::BestEffort,
        CliOsSandboxPolicyMode::Required => OsSandboxPolicyMode::Required,
    }
}

fn map_cli_os_sandbox_docker_network(value: CliOsSandboxDockerNetwork) -> OsSandboxDockerNetwork {
    match value {
        CliOsSandboxDockerNetwork::None => OsSandboxDockerNetwork::None,
        CliOsSandboxDockerNetwork::Bridge => OsSandboxDockerNetwork::Bridge,
        CliOsSandboxDockerNetwork::Host => OsSandboxDockerNetwork::Host,
    }
}

fn parse_docker_cpu_limit(raw: &str) -> Result<f32> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("--os-sandbox-docker-cpus cannot be empty"));
    }
    let value = trimmed.parse::<f32>().map_err(|error| {
        anyhow!(
            "invalid --os-sandbox-docker-cpus value '{}': {}",
            trimmed,
            error
        )
    })?;
    if !value.is_finite() || value <= 0.0 {
        return Err(anyhow!(
            "--os-sandbox-docker-cpus must be a finite number greater than 0"
        ));
    }
    Ok(value)
}

fn parse_docker_env_allowlist(raw: &[String]) -> Result<Vec<String>> {
    let mut values = Vec::new();
    for item in raw {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        for token in trimmed.split(',') {
            let token = token.trim();
            if token.is_empty() {
                continue;
            }
            validate_env_name(token)?;
            if !values.iter().any(|existing| existing == token) {
                values.push(token.to_string());
            }
        }
    }
    Ok(values)
}

fn validate_env_name(value: &str) -> Result<()> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(anyhow!("environment variable name cannot be empty"));
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return Err(anyhow!(
            "invalid environment variable name '{}': first character must be [A-Za-z_]",
            value
        ));
    }
    if chars.any(|ch| !(ch == '_' || ch.is_ascii_alphanumeric())) {
        return Err(anyhow!(
            "invalid environment variable name '{}': only [A-Za-z0-9_] characters are supported",
            value
        ));
    }
    Ok(())
}

fn map_cli_tool_policy_preset(value: CliToolPolicyPreset) -> crate::tools::ToolPolicyPreset {
    match value {
        CliToolPolicyPreset::Permissive => crate::tools::ToolPolicyPreset::Permissive,
        CliToolPolicyPreset::Balanced => crate::tools::ToolPolicyPreset::Balanced,
        CliToolPolicyPreset::Strict => crate::tools::ToolPolicyPreset::Strict,
        CliToolPolicyPreset::Hardened => crate::tools::ToolPolicyPreset::Hardened,
    }
}

#[cfg(test)]
mod tests {
    use super::{build_tool_policy, tool_policy_to_json};
    use crate::tools::ToolPolicy;
    use clap::Parser;
    use std::ffi::OsString;
    use std::sync::{Mutex, OnceLock};
    use tau_cli::cli_args::Cli;
    use tempfile::tempdir;

    #[test]
    fn unit_tool_policy_json_exposes_protected_path_controls() {
        let temp = tempdir().expect("tempdir");
        let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
        policy.allow_protected_path_mutations = true;
        let payload = tool_policy_to_json(&policy);

        assert_eq!(payload["schema_version"], 13);
        assert_eq!(payload["allow_protected_path_mutations"], true);
        assert_eq!(payload["memory_search_default_limit"], 5);
        assert_eq!(payload["memory_search_max_limit"], 50);
        assert_eq!(payload["memory_embedding_dimensions"], 128);
        assert_eq!(payload["memory_embedding_provider"], "local");
        assert_eq!(payload["memory_embedding_model"], serde_json::Value::Null);
        assert_eq!(
            payload["memory_embedding_api_base"],
            serde_json::Value::Null
        );
        assert_eq!(payload["memory_embedding_api_key_present"], false);
        assert_eq!(payload["memory_embedding_timeout_ms"], 10_000);
        assert_eq!(payload["memory_enable_hybrid_retrieval"], false);
        assert!(
            (payload["memory_bm25_k1"]
                .as_f64()
                .expect("memory_bm25_k1 as f64")
                - 1.2)
                .abs()
                < 1e-6
        );
        assert!(
            (payload["memory_bm25_b"]
                .as_f64()
                .expect("memory_bm25_b as f64")
                - 0.75)
                .abs()
                < 1e-6
        );
        assert_eq!(payload["memory_bm25_min_score"], 0.0);
        assert_eq!(payload["memory_rrf_k"], 60);
        assert_eq!(payload["memory_rrf_vector_weight"], 1.0);
        assert_eq!(payload["memory_rrf_lexical_weight"], 1.0);
        assert_eq!(payload["memory_enable_embedding_migration"], true);
        assert_eq!(payload["memory_benchmark_against_hash"], false);
        assert_eq!(payload["memory_benchmark_against_vector_only"], false);
        let min_similarity = payload["memory_min_similarity"]
            .as_f64()
            .expect("memory_min_similarity as f64");
        assert!((min_similarity - 0.55).abs() < 1e-6);
        let defaults = &payload["memory_default_importance_profile"];
        assert!((defaults["identity"].as_f64().expect("identity as f64") - 1.0).abs() < 1e-6);
        assert!((defaults["goal"].as_f64().expect("goal as f64") - 0.9).abs() < 1e-6);
        assert!((defaults["decision"].as_f64().expect("decision as f64") - 0.85).abs() < 1e-6);
        assert!((defaults["todo"].as_f64().expect("todo as f64") - 0.8).abs() < 1e-6);
        assert!((defaults["preference"].as_f64().expect("preference as f64") - 0.7).abs() < 1e-6);
        assert!((defaults["fact"].as_f64().expect("fact as f64") - 0.65).abs() < 1e-6);
        assert!((defaults["event"].as_f64().expect("event as f64") - 0.55).abs() < 1e-6);
        assert!(
            (defaults["observation"]
                .as_f64()
                .expect("observation as f64")
                - 0.3)
                .abs()
                < 1e-6
        );
        assert!(payload["memory_state_dir"]
            .as_str()
            .map(|value| value.ends_with(".tau/memory"))
            .unwrap_or(false));
        assert_eq!(payload["jobs_enabled"], true);
        assert!(payload["jobs_state_dir"]
            .as_str()
            .map(|value| value.ends_with(".tau/jobs"))
            .unwrap_or(false));
        assert_eq!(payload["jobs_list_default_limit"], 20);
        assert_eq!(payload["jobs_list_max_limit"], 200);
        assert_eq!(payload["jobs_default_timeout_ms"], 30_000);
        assert_eq!(payload["jobs_max_timeout_ms"], 900_000);
        assert!(payload["jobs_channel_store_root"]
            .as_str()
            .map(|value| value.ends_with(".tau/channel-store"))
            .unwrap_or(false));
        assert_eq!(payload["os_sandbox_policy_mode"], "best-effort");
        assert_eq!(payload["os_sandbox_docker_enabled"], false);
        assert_eq!(payload["os_sandbox_docker_image"], "debian:stable-slim");
        assert_eq!(payload["os_sandbox_docker_network"], "none");
        assert_eq!(payload["os_sandbox_docker_memory_mb"], 256);
        assert_eq!(payload["os_sandbox_docker_cpu_limit"], 1.0);
        assert_eq!(payload["os_sandbox_docker_pids_limit"], 256);
        assert_eq!(payload["os_sandbox_docker_read_only_rootfs"], true);
        assert_eq!(
            payload["os_sandbox_docker_env_allowlist"]
                .as_array()
                .map(std::vec::Vec::len),
            Some(0)
        );
        assert_eq!(payload["http_timeout_ms"], 20_000);
        assert_eq!(payload["http_max_response_bytes"], 256_000);
        assert_eq!(payload["http_max_redirects"], 5);
        assert_eq!(payload["http_allow_http"], false);
        assert_eq!(payload["http_allow_private_network"], false);
        assert_eq!(payload["tool_builder_enabled"], false);
        assert!(payload["tool_builder_output_root"]
            .as_str()
            .map(|value| value.ends_with(".tau/generated-tools"))
            .unwrap_or(false));
        assert!(payload["tool_builder_extension_root"]
            .as_str()
            .map(|value| value.ends_with(".tau/extensions/generated"))
            .unwrap_or(false));
        assert_eq!(payload["tool_builder_max_attempts"], 3);
        assert!(payload["protected_paths"]
            .as_array()
            .map(|paths| {
                paths.iter().any(|path| {
                    path.as_str()
                        .map(|value| value.ends_with("AGENTS.md"))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false));
    }

    #[test]
    fn integration_build_tool_policy_reads_memory_embedding_env_without_exposing_keys() {
        let _guard = env_lock().lock().expect("env lock");
        let vars = [
            "TAU_MEMORY_EMBEDDING_PROVIDER",
            "TAU_MEMORY_EMBEDDING_MODEL",
            "TAU_MEMORY_EMBEDDING_API_BASE",
            "TAU_MEMORY_EMBEDDING_API_KEY",
            "TAU_MEMORY_EMBEDDING_TIMEOUT_MS",
            "TAU_MEMORY_ENABLE_HYBRID_RETRIEVAL",
            "TAU_MEMORY_BM25_K1",
            "TAU_MEMORY_BM25_B",
            "TAU_MEMORY_BM25_MIN_SCORE",
            "TAU_MEMORY_RRF_K",
            "TAU_MEMORY_RRF_VECTOR_WEIGHT",
            "TAU_MEMORY_RRF_LEXICAL_WEIGHT",
            "TAU_MEMORY_ENABLE_EMBEDDING_MIGRATION",
            "TAU_MEMORY_BENCHMARK_AGAINST_HASH",
            "TAU_MEMORY_BENCHMARK_AGAINST_VECTOR_ONLY",
            "TAU_MEMORY_DEFAULT_IMPORTANCE_IDENTITY",
            "TAU_MEMORY_DEFAULT_IMPORTANCE_GOAL",
            "TAU_MEMORY_DEFAULT_IMPORTANCE_DECISION",
            "TAU_MEMORY_DEFAULT_IMPORTANCE_TODO",
            "TAU_MEMORY_DEFAULT_IMPORTANCE_PREFERENCE",
            "TAU_MEMORY_DEFAULT_IMPORTANCE_FACT",
            "TAU_MEMORY_DEFAULT_IMPORTANCE_EVENT",
            "TAU_MEMORY_DEFAULT_IMPORTANCE_OBSERVATION",
        ];
        let _snapshot = EnvSnapshot::capture(&vars);
        for name in vars {
            std::env::remove_var(name);
        }

        std::env::set_var("TAU_MEMORY_EMBEDDING_PROVIDER", "openai-compatible");
        std::env::set_var("TAU_MEMORY_EMBEDDING_MODEL", "text-embedding-3-small");
        std::env::set_var(
            "TAU_MEMORY_EMBEDDING_API_BASE",
            "https://embeddings.example/v1",
        );
        std::env::set_var("TAU_MEMORY_EMBEDDING_API_KEY", "secret");
        std::env::set_var("TAU_MEMORY_EMBEDDING_TIMEOUT_MS", "2500");
        std::env::set_var("TAU_MEMORY_ENABLE_HYBRID_RETRIEVAL", "true");
        std::env::set_var("TAU_MEMORY_BM25_K1", "1.6");
        std::env::set_var("TAU_MEMORY_BM25_B", "0.4");
        std::env::set_var("TAU_MEMORY_BM25_MIN_SCORE", "0.2");
        std::env::set_var("TAU_MEMORY_RRF_K", "42");
        std::env::set_var("TAU_MEMORY_RRF_VECTOR_WEIGHT", "1.5");
        std::env::set_var("TAU_MEMORY_RRF_LEXICAL_WEIGHT", "0.8");
        std::env::set_var("TAU_MEMORY_ENABLE_EMBEDDING_MIGRATION", "false");
        std::env::set_var("TAU_MEMORY_BENCHMARK_AGAINST_HASH", "true");
        std::env::set_var("TAU_MEMORY_BENCHMARK_AGAINST_VECTOR_ONLY", "true");

        let cli = parse_cli_with_stack();
        let policy = build_tool_policy(&cli).expect("build tool policy");
        assert_eq!(
            policy.memory_embedding_provider.as_deref(),
            Some("openai-compatible")
        );
        assert_eq!(
            policy.memory_embedding_model.as_deref(),
            Some("text-embedding-3-small")
        );
        assert_eq!(
            policy.memory_embedding_api_base.as_deref(),
            Some("https://embeddings.example/v1")
        );
        assert_eq!(policy.memory_embedding_api_key.as_deref(), Some("secret"));
        assert_eq!(policy.memory_embedding_timeout_ms, 2_500);
        assert!(policy.memory_enable_hybrid_retrieval);
        assert!((policy.memory_bm25_k1 - 1.6).abs() < 1e-6);
        assert!((policy.memory_bm25_b - 0.4).abs() < 1e-6);
        assert!((policy.memory_bm25_min_score - 0.2).abs() < 1e-6);
        assert_eq!(policy.memory_rrf_k, 42);
        assert!((policy.memory_rrf_vector_weight - 1.5).abs() < 1e-6);
        assert!((policy.memory_rrf_lexical_weight - 0.8).abs() < 1e-6);
        assert!(!policy.memory_enable_embedding_migration);
        assert!(policy.memory_benchmark_against_hash);
        assert!(policy.memory_benchmark_against_vector_only);

        let payload = tool_policy_to_json(&policy);
        assert_eq!(payload["memory_embedding_api_key_present"], true);
        assert_eq!(payload["memory_enable_hybrid_retrieval"], true);
        assert!(
            (payload["memory_bm25_k1"]
                .as_f64()
                .expect("memory_bm25_k1 as f64")
                - 1.6)
                .abs()
                < 1e-6
        );
        assert_eq!(payload["memory_rrf_k"], 42);
        assert_eq!(payload["memory_benchmark_against_vector_only"], true);
        assert!(!payload
            .as_object()
            .map(|object| object.contains_key("memory_embedding_api_key"))
            .unwrap_or(false));
    }

    #[test]
    fn spec_2589_c01_tool_policy_parses_memory_default_importance_overrides() {
        let _guard = env_lock().lock().expect("env lock");
        let vars = [
            "TAU_MEMORY_DEFAULT_IMPORTANCE_IDENTITY",
            "TAU_MEMORY_DEFAULT_IMPORTANCE_GOAL",
            "TAU_MEMORY_DEFAULT_IMPORTANCE_DECISION",
            "TAU_MEMORY_DEFAULT_IMPORTANCE_TODO",
            "TAU_MEMORY_DEFAULT_IMPORTANCE_PREFERENCE",
            "TAU_MEMORY_DEFAULT_IMPORTANCE_FACT",
            "TAU_MEMORY_DEFAULT_IMPORTANCE_EVENT",
            "TAU_MEMORY_DEFAULT_IMPORTANCE_OBSERVATION",
        ];
        let _snapshot = EnvSnapshot::capture(&vars);
        for name in vars {
            std::env::remove_var(name);
        }

        std::env::set_var("TAU_MEMORY_DEFAULT_IMPORTANCE_IDENTITY", "0.91");
        std::env::set_var("TAU_MEMORY_DEFAULT_IMPORTANCE_OBSERVATION", "0.22");
        std::env::set_var("TAU_MEMORY_DEFAULT_IMPORTANCE_FACT", "0.6");

        let cli = parse_cli_with_stack();
        let policy = build_tool_policy(&cli).expect("build tool policy");
        assert!((policy.memory_default_importance_profile.identity - 0.91).abs() < 1e-6);
        assert!((policy.memory_default_importance_profile.observation - 0.22).abs() < 1e-6);
        assert!((policy.memory_default_importance_profile.fact - 0.6).abs() < 1e-6);
        assert!((policy.memory_default_importance_profile.goal - 0.9).abs() < 1e-6);

        let payload = tool_policy_to_json(&policy);
        let identity = payload["memory_default_importance_profile"]["identity"]
            .as_f64()
            .expect("identity as f64");
        let observation = payload["memory_default_importance_profile"]["observation"]
            .as_f64()
            .expect("observation as f64");
        let fact = payload["memory_default_importance_profile"]["fact"]
            .as_f64()
            .expect("fact as f64");
        assert!((identity - 0.91).abs() < 1e-6);
        assert!((observation - 0.22).abs() < 1e-6);
        assert!((fact - 0.6).abs() < 1e-6);
    }

    #[test]
    fn regression_2589_c04_memory_write_rejects_out_of_range_configured_defaults() {
        let _guard = env_lock().lock().expect("env lock");
        let vars = ["TAU_MEMORY_DEFAULT_IMPORTANCE_IDENTITY"];
        let _snapshot = EnvSnapshot::capture(&vars);
        for name in vars {
            std::env::remove_var(name);
        }

        std::env::set_var("TAU_MEMORY_DEFAULT_IMPORTANCE_IDENTITY", "1.5");
        let cli = parse_cli_with_stack();
        let error = build_tool_policy(&cli).expect_err("out-of-range defaults must fail closed");
        assert!(
            error
                .to_string()
                .contains("TAU_MEMORY_DEFAULT_IMPORTANCE_IDENTITY"),
            "error must identify invalid override env var: {error}"
        );
    }

    #[test]
    fn integration_build_tool_policy_defaults_memory_embedding_provider_local() {
        let _guard = env_lock().lock().expect("env lock");
        let vars = [
            "TAU_MEMORY_EMBEDDING_PROVIDER",
            "TAU_MEMORY_EMBEDDING_MODEL",
            "TAU_MEMORY_EMBEDDING_API_BASE",
            "TAU_MEMORY_EMBEDDING_API_KEY",
        ];
        let _snapshot = EnvSnapshot::capture(&vars);
        for name in vars {
            std::env::remove_var(name);
        }

        let cli = parse_cli_with_stack();
        let policy = build_tool_policy(&cli).expect("build tool policy");
        assert_eq!(policy.memory_embedding_provider.as_deref(), Some("local"));

        let payload = tool_policy_to_json(&policy);
        assert_eq!(payload["memory_embedding_provider"], "local");
        assert_eq!(
            payload["memory_embedding_api_base"],
            serde_json::Value::Null
        );
        assert_eq!(payload["memory_embedding_api_key_present"], false);
    }

    #[test]
    fn regression_build_tool_policy_remote_provider_uses_cli_api_fallback_fields() {
        let _guard = env_lock().lock().expect("env lock");
        let vars = [
            "TAU_API_BASE",
            "TAU_API_KEY",
            "OPENAI_API_KEY",
            "TAU_MEMORY_EMBEDDING_PROVIDER",
            "TAU_MEMORY_EMBEDDING_MODEL",
            "TAU_MEMORY_EMBEDDING_API_BASE",
            "TAU_MEMORY_EMBEDDING_API_KEY",
        ];
        let _snapshot = EnvSnapshot::capture(&vars);
        for name in vars {
            std::env::remove_var(name);
        }
        std::env::set_var("TAU_API_BASE", "https://fallback.example/v1");
        std::env::set_var("OPENAI_API_KEY", "fallback-secret");
        std::env::set_var("TAU_MEMORY_EMBEDDING_PROVIDER", "openai-compatible");
        std::env::set_var("TAU_MEMORY_EMBEDDING_MODEL", "text-embedding-3-small");

        let cli = parse_cli_with_stack();
        let policy = build_tool_policy(&cli).expect("build tool policy");
        assert_eq!(
            policy.memory_embedding_api_base.as_deref(),
            Some("https://fallback.example/v1")
        );
        assert_eq!(
            policy.memory_embedding_api_key.as_deref(),
            Some("fallback-secret")
        );
    }

    #[test]
    fn functional_build_tool_policy_applies_docker_sandbox_settings() {
        let cli = parse_cli_with_stack_args(vec![
            "tau-rs",
            "--os-sandbox-docker-enabled=true",
            "--os-sandbox-docker-image",
            "ubuntu:24.04",
            "--os-sandbox-docker-network",
            "bridge",
            "--os-sandbox-docker-memory-mb",
            "640",
            "--os-sandbox-docker-cpus",
            "2.25",
            "--os-sandbox-docker-pids-limit",
            "96",
            "--os-sandbox-docker-read-only-rootfs=false",
            "--os-sandbox-docker-env=OPENAI_API_KEY,TAU_TOKEN",
        ]);
        let policy = build_tool_policy(&cli).expect("build tool policy");
        assert!(policy.os_sandbox_docker_enabled);
        assert_eq!(policy.os_sandbox_docker_image, "ubuntu:24.04");
        assert_eq!(
            policy.os_sandbox_docker_network,
            crate::tools::OsSandboxDockerNetwork::Bridge
        );
        assert_eq!(policy.os_sandbox_docker_memory_mb, 640);
        assert!((policy.os_sandbox_docker_cpu_limit - 2.25).abs() < 1e-6);
        assert_eq!(policy.os_sandbox_docker_pids_limit, 96);
        assert!(!policy.os_sandbox_docker_read_only_rootfs);
        assert_eq!(
            policy.os_sandbox_docker_env_allowlist,
            vec!["OPENAI_API_KEY".to_string(), "TAU_TOKEN".to_string()]
        );
    }

    #[test]
    fn integration_tool_policy_json_exposes_docker_sandbox_settings() {
        let cli = parse_cli_with_stack_args(vec![
            "tau-rs",
            "--os-sandbox-docker-enabled=true",
            "--os-sandbox-docker-image",
            "ubuntu:24.04",
            "--os-sandbox-docker-network",
            "host",
            "--os-sandbox-docker-memory-mb",
            "768",
            "--os-sandbox-docker-cpus",
            "3.0",
            "--os-sandbox-docker-pids-limit",
            "144",
            "--os-sandbox-docker-env",
            "OPENAI_API_KEY",
        ]);
        let policy = build_tool_policy(&cli).expect("build tool policy");
        let payload = tool_policy_to_json(&policy);
        assert_eq!(payload["os_sandbox_docker_enabled"], true);
        assert_eq!(payload["os_sandbox_docker_image"], "ubuntu:24.04");
        assert_eq!(payload["os_sandbox_docker_network"], "host");
        assert_eq!(payload["os_sandbox_docker_memory_mb"], 768);
        assert_eq!(payload["os_sandbox_docker_cpu_limit"], 3.0);
        assert_eq!(payload["os_sandbox_docker_pids_limit"], 144);
        assert_eq!(
            payload["os_sandbox_docker_env_allowlist"],
            serde_json::json!(["OPENAI_API_KEY"])
        );
    }

    #[test]
    fn functional_build_tool_policy_applies_tool_builder_settings() {
        let cli = parse_cli_with_stack_args(vec![
            "tau-rs",
            "--tool-builder-enabled",
            "--tool-builder-output-root",
            ".tau/generated-artifacts",
            "--tool-builder-extension-root",
            ".tau/extensions/generated-runtime",
            "--tool-builder-max-attempts",
            "6",
        ]);
        let policy = build_tool_policy(&cli).expect("build tool policy");
        assert!(policy.tool_builder_enabled);
        assert!(policy
            .tool_builder_output_root
            .ends_with(".tau/generated-artifacts"));
        assert!(policy
            .tool_builder_extension_root
            .ends_with(".tau/extensions/generated-runtime"));
        assert_eq!(policy.tool_builder_max_attempts, 6);
    }

    #[test]
    fn regression_build_tool_policy_rejects_invalid_docker_cpu_limit() {
        let cli = parse_cli_with_stack_args(vec![
            "tau-rs",
            "--os-sandbox-docker-enabled=true",
            "--os-sandbox-docker-cpus",
            "0",
        ]);
        let error = build_tool_policy(&cli).expect_err("invalid docker cpus should fail");
        assert!(error.to_string().contains("--os-sandbox-docker-cpus"));
    }

    #[test]
    fn regression_build_tool_policy_rejects_invalid_docker_env_allowlist_name() {
        let cli = parse_cli_with_stack_args(vec![
            "tau-rs",
            "--os-sandbox-docker-enabled=true",
            "--os-sandbox-docker-env",
            "BAD-NAME",
        ]);
        let error = build_tool_policy(&cli).expect_err("invalid docker env name should fail");
        assert!(error
            .to_string()
            .contains("invalid environment variable name"));
    }

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn parse_cli_with_stack() -> Cli {
        std::thread::Builder::new()
            .name("tau-cli-parse".to_string())
            .stack_size(16 * 1024 * 1024)
            .spawn(|| Cli::parse_from(["tau-rs"]))
            .expect("spawn cli parse thread")
            .join()
            .expect("join cli parse thread")
    }

    fn parse_cli_with_stack_args(args: Vec<&'static str>) -> Cli {
        std::thread::Builder::new()
            .name("tau-cli-parse-args".to_string())
            .stack_size(16 * 1024 * 1024)
            .spawn(move || Cli::parse_from(args))
            .expect("spawn cli parse args thread")
            .join()
            .expect("join cli parse args thread")
    }

    struct EnvSnapshot {
        values: Vec<(String, Option<OsString>)>,
    }

    impl EnvSnapshot {
        fn capture(names: &[&str]) -> Self {
            Self {
                values: names
                    .iter()
                    .map(|name| ((*name).to_string(), std::env::var_os(name)))
                    .collect(),
            }
        }
    }

    impl Drop for EnvSnapshot {
        fn drop(&mut self) {
            for (name, value) in self.values.drain(..) {
                match value {
                    Some(previous) => std::env::set_var(name, previous),
                    None => std::env::remove_var(name),
                }
            }
        }
    }
}
