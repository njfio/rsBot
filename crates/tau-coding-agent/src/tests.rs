use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    future::{pending, ready},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use clap::Parser;
use httpmock::prelude::*;
use sha2::{Digest, Sha256};
use tau_agent_core::{Agent, AgentConfig, AgentEvent, ToolExecutionResult};
use tau_ai::{
    ChatRequest, ChatResponse, ChatUsage, ContentBlock, LlmClient, Message, MessageRole, ModelRef,
    Provider, TauAiError,
};
use tempfile::tempdir;
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::sleep;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use super::{
    apply_trust_root_mutations, branch_alias_path_for_session, build_auth_command_config,
    build_doctor_command_config, build_profile_defaults, build_provider_client, build_tool_policy,
    command_file_error_mode_label, compose_startup_system_prompt, compute_session_entry_depths,
    compute_session_stats, current_unix_timestamp, decrypt_credential_store_secret,
    default_macro_config_path, default_profile_store_path, default_skills_lock_path,
    derive_skills_prune_candidates, encrypt_credential_store_secret, ensure_non_empty_text,
    escape_graph_label, execute_auth_command, execute_branch_alias_command,
    execute_channel_store_admin_command, execute_command_file, execute_doctor_command,
    execute_integration_auth_command, execute_macro_command, execute_package_activate_command,
    execute_package_activate_on_startup, execute_package_conflicts_command,
    execute_package_install_command, execute_package_list_command, execute_package_remove_command,
    execute_package_rollback_command, execute_package_show_command, execute_package_update_command,
    execute_package_validate_command, execute_profile_command, execute_rpc_capabilities_command,
    execute_rpc_dispatch_frame_command, execute_rpc_dispatch_ndjson_command,
    execute_rpc_serve_ndjson_command, execute_rpc_validate_frame_command,
    execute_session_bookmark_command, execute_session_diff_command,
    execute_session_graph_export_command, execute_session_search_command,
    execute_session_stats_command, execute_skills_list_command, execute_skills_lock_diff_command,
    execute_skills_lock_write_command, execute_skills_prune_command, execute_skills_search_command,
    execute_skills_show_command, execute_skills_sync_command, execute_skills_trust_add_command,
    execute_skills_trust_list_command, execute_skills_trust_revoke_command,
    execute_skills_trust_rotate_command, execute_skills_verify_command, execute_startup_preflight,
    format_id_list, format_remap_ids, handle_command, handle_command_with_session_import_mode,
    initialize_session, is_retryable_provider_error, load_branch_aliases, load_credential_store,
    load_macro_file, load_profile_store, load_session_bookmarks, load_trust_root_records,
    parse_auth_command, parse_branch_alias_command, parse_command, parse_command_file,
    parse_doctor_command_args, parse_integration_auth_command, parse_macro_command,
    parse_numbered_plan_steps, parse_profile_command, parse_sandbox_command_tokens,
    parse_session_bookmark_command, parse_session_diff_args, parse_session_search_args,
    parse_session_stats_args, parse_skills_lock_diff_args, parse_skills_prune_args,
    parse_skills_search_args, parse_skills_trust_list_args, parse_skills_trust_mutation_args,
    parse_skills_verify_args, parse_trust_rotation_spec, parse_trusted_root_spec,
    percentile_duration_ms, provider_auth_capability, refresh_provider_access_token,
    register_runtime_extension_tool_hook_subscriber, render_audit_summary, render_command_help,
    render_doctor_report, render_doctor_report_json, render_help_overview, render_macro_list,
    render_macro_show, render_profile_diffs, render_profile_list, render_profile_show,
    render_session_diff, render_session_graph_dot, render_session_graph_mermaid,
    render_session_stats, render_session_stats_json, render_skills_list,
    render_skills_lock_diff_drift, render_skills_lock_diff_in_sync,
    render_skills_lock_write_success, render_skills_search, render_skills_show,
    render_skills_sync_drift_details, render_skills_trust_list, render_skills_verify_report,
    resolve_credential_store_encryption_mode, resolve_fallback_models, resolve_prompt_input,
    resolve_prunable_skill_file_name, resolve_secret_from_cli_or_store_id,
    resolve_session_graph_format, resolve_skill_trust_roots, resolve_skills_lock_path,
    resolve_store_backed_provider_credential, resolve_system_prompt, rpc_capabilities_payload,
    run_doctor_checks, run_plan_first_prompt, run_plan_first_prompt_with_policy_context,
    run_prompt_with_cancellation, save_branch_aliases, save_credential_store, save_macro_file,
    save_profile_store, save_session_bookmarks, search_session_entries,
    session_bookmark_path_for_session, session_message_preview, shared_lineage_prefix_depth,
    stream_text_chunks, summarize_audit_file, tool_audit_event_json, tool_policy_to_json,
    trust_record_status, unknown_command_message, validate_branch_alias_name,
    validate_event_webhook_ingest_cli, validate_events_runner_cli,
    validate_github_issues_bridge_cli, validate_macro_command_entry, validate_macro_name,
    validate_profile_name, validate_rpc_frame_file, validate_session_file,
    validate_skills_prune_file_name, validate_slack_bridge_cli, AuthCommand, AuthCommandConfig,
    BranchAliasCommand, BranchAliasFile, Cli, CliBashProfile, CliCommandFileErrorMode,
    CliCredentialStoreEncryptionMode, CliOrchestratorMode, CliOsSandboxMode, CliProviderAuthMode,
    CliSessionImportMode, CliToolPolicyPreset, CliWebhookSignatureAlgorithm, ClientRoute,
    CommandAction, CommandExecutionContext, CommandFileEntry, CommandFileReport,
    CredentialStoreData, CredentialStoreEncryptionMode, DoctorCheckResult, DoctorCommandConfig,
    DoctorCommandOutputFormat, DoctorProviderKeyStatus, DoctorStatus, FallbackRoutingClient,
    IntegrationAuthCommand, IntegrationCredentialStoreRecord, MacroCommand, MacroFile,
    ProfileCommand, ProfileDefaults, ProfileStoreFile, PromptRunStatus, PromptTelemetryLogger,
    ProviderAuthMethod, ProviderCredentialStoreRecord, RenderOptions, RuntimeExtensionHooksConfig,
    SessionBookmarkCommand, SessionBookmarkFile, SessionDiffEntry, SessionDiffReport,
    SessionGraphFormat, SessionRuntime, SessionSearchArgs, SessionStats, SessionStatsOutputFormat,
    SkillsPruneMode, SkillsSyncCommandConfig, SkillsVerifyEntry, SkillsVerifyReport,
    SkillsVerifyStatus, SkillsVerifySummary, SkillsVerifyTrustSummary, ToolAuditLogger,
    TrustedRootRecord, BRANCH_ALIAS_SCHEMA_VERSION, BRANCH_ALIAS_USAGE, MACRO_SCHEMA_VERSION,
    MACRO_USAGE, PROFILE_SCHEMA_VERSION, PROFILE_USAGE, SESSION_BOOKMARK_SCHEMA_VERSION,
    SESSION_BOOKMARK_USAGE, SESSION_SEARCH_DEFAULT_RESULTS, SESSION_SEARCH_PREVIEW_CHARS,
    SKILLS_PRUNE_USAGE, SKILLS_TRUST_ADD_USAGE, SKILLS_TRUST_LIST_USAGE, SKILLS_VERIFY_USAGE,
};
use crate::auth_commands::{
    auth_availability_counts, auth_mode_counts, auth_provider_counts, auth_revoked_counts,
    auth_source_kind, auth_source_kind_counts, auth_state_counts, auth_status_row_for_provider,
    format_auth_state_counts, AuthMatrixAvailabilityFilter, AuthMatrixModeSupportFilter,
    AuthRevokedFilter, AuthSourceKindFilter,
};
use crate::extension_manifest::discover_extension_runtime_registrations;
use crate::provider_api_key_candidates_with_inputs;
use crate::provider_credentials::provider_auth_snapshot_for_status;
use crate::resolve_api_key;
use crate::session::{SessionImportMode, SessionStore};
use crate::tools::{register_extension_tools, BashCommandProfile, OsSandboxMode, ToolPolicyPreset};
use crate::{default_model_catalog_cache_path, ModelCatalog, MODELS_LIST_USAGE, MODEL_SHOW_USAGE};
use crate::{
    execute_extension_exec_command, execute_extension_list_command, execute_extension_show_command,
    execute_extension_validate_command,
};

static AUTH_ENV_TEST_LOCK: Mutex<()> = Mutex::new(());

fn snapshot_env_vars(keys: &[&str]) -> Vec<(String, Option<String>)> {
    keys.iter()
        .map(|key| ((*key).to_string(), std::env::var(key).ok()))
        .collect()
}

fn restore_env_vars(snapshot: Vec<(String, Option<String>)>) {
    for (key, value) in snapshot {
        if let Some(value) = value {
            std::env::set_var(key, value);
        } else {
            std::env::remove_var(key);
        }
    }
}

#[cfg(unix)]
fn write_mock_codex_script(dir: &Path, body: &str) -> PathBuf {
    let script = dir.join("mock-codex.sh");
    let content = format!("#!/bin/sh\nset -eu\n{body}\n");
    std::fs::write(&script, content).expect("write mock codex script");
    let mut perms = std::fs::metadata(&script)
        .expect("mock codex metadata")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).expect("chmod mock codex script");
    script
}

#[cfg(unix)]
fn write_mock_gemini_script(dir: &Path, body: &str) -> PathBuf {
    let script = dir.join("mock-gemini.sh");
    let content = format!("#!/bin/sh\nset -eu\n{body}\n");
    std::fs::write(&script, content).expect("write mock gemini script");
    let mut perms = std::fs::metadata(&script)
        .expect("mock gemini metadata")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).expect("chmod mock gemini script");
    script
}

#[cfg(unix)]
fn write_mock_claude_script(dir: &Path, body: &str) -> PathBuf {
    let script = dir.join("mock-claude.sh");
    let content = format!("#!/bin/sh\nset -eu\n{body}\n");
    std::fs::write(&script, content).expect("write mock claude script");
    let mut perms = std::fs::metadata(&script)
        .expect("mock claude metadata")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).expect("chmod mock claude script");
    script
}

#[cfg(unix)]
fn write_mock_gcloud_script(dir: &Path, body: &str) -> PathBuf {
    let script = dir.join("mock-gcloud.sh");
    let content = format!("#!/bin/sh\nset -eu\n{body}\n");
    std::fs::write(&script, content).expect("write mock gcloud script");
    let mut perms = std::fs::metadata(&script)
        .expect("mock gcloud metadata")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).expect("chmod mock gcloud script");
    script
}

struct NoopClient;

#[async_trait]
impl LlmClient for NoopClient {
    async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
        Err(TauAiError::InvalidResponse(
            "noop client should not be called".to_string(),
        ))
    }
}

struct SuccessClient;

#[async_trait]
impl LlmClient for SuccessClient {
    async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
        Ok(ChatResponse {
            message: tau_ai::Message::assistant_text("done"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        })
    }
}

struct SlowClient;

#[async_trait]
impl LlmClient for SlowClient {
    async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
        sleep(Duration::from_secs(5)).await;
        Ok(ChatResponse {
            message: tau_ai::Message::assistant_text("slow"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        })
    }
}

struct QueueClient {
    responses: AsyncMutex<VecDeque<ChatResponse>>,
}

#[async_trait]
impl LlmClient for QueueClient {
    async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
        let mut responses = self.responses.lock().await;
        responses
            .pop_front()
            .ok_or_else(|| TauAiError::InvalidResponse("mock response queue is empty".to_string()))
    }
}

struct SequenceClient {
    outcomes: AsyncMutex<VecDeque<Result<ChatResponse, TauAiError>>>,
}

#[async_trait]
impl LlmClient for SequenceClient {
    async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
        let mut outcomes = self.outcomes.lock().await;
        outcomes.pop_front().unwrap_or_else(|| {
            Err(TauAiError::InvalidResponse(
                "mock outcome queue is empty".to_string(),
            ))
        })
    }
}

fn test_render_options() -> RenderOptions {
    RenderOptions {
        stream_output: false,
        stream_delay_ms: 0,
    }
}

fn test_tool_policy_json() -> serde_json::Value {
    serde_json::json!({
        "allowed_roots": [],
        "bash_profile": "balanced",
    })
}

fn test_chat_request() -> ChatRequest {
    ChatRequest {
        model: "placeholder-model".to_string(),
        messages: vec![Message::user("hello")],
        tools: vec![],
        max_tokens: None,
        temperature: None,
    }
}

fn test_cli() -> Cli {
    Cli {
        model: "openai/gpt-4o-mini".to_string(),
        fallback_model: vec![],
        api_base: "https://api.openai.com/v1".to_string(),
        azure_openai_api_version: "2024-10-21".to_string(),
        model_catalog_url: None,
        model_catalog_cache: default_model_catalog_cache_path(),
        model_catalog_offline: false,
        model_catalog_stale_after_hours: 24,
        anthropic_api_base: "https://api.anthropic.com/v1".to_string(),
        google_api_base: "https://generativelanguage.googleapis.com/v1beta".to_string(),
        api_key: None,
        openai_api_key: None,
        anthropic_api_key: None,
        google_api_key: None,
        openai_auth_mode: CliProviderAuthMode::ApiKey,
        openai_codex_backend: true,
        openai_codex_cli: "codex".to_string(),
        openai_codex_args: vec![],
        openai_codex_timeout_ms: 120_000,
        anthropic_auth_mode: CliProviderAuthMode::ApiKey,
        anthropic_claude_backend: true,
        anthropic_claude_cli: "claude".to_string(),
        anthropic_claude_args: vec![],
        anthropic_claude_timeout_ms: 120_000,
        google_auth_mode: CliProviderAuthMode::ApiKey,
        google_gemini_backend: true,
        google_gemini_cli: "gemini".to_string(),
        google_gcloud_cli: "gcloud".to_string(),
        google_gemini_args: vec![],
        google_gemini_timeout_ms: 120_000,
        credential_store: PathBuf::from(".tau/credentials.json"),
        credential_store_key: None,
        credential_store_encryption: CliCredentialStoreEncryptionMode::Auto,
        system_prompt: "sys".to_string(),
        system_prompt_file: None,
        skills_dir: PathBuf::from(".tau/skills"),
        skills: vec![],
        install_skill: vec![],
        install_skill_url: vec![],
        install_skill_sha256: vec![],
        skill_registry_url: None,
        skill_registry_sha256: None,
        install_skill_from_registry: vec![],
        skills_cache_dir: None,
        skills_offline: false,
        skill_trust_root: vec![],
        skill_trust_root_file: None,
        skill_trust_add: vec![],
        skill_trust_revoke: vec![],
        skill_trust_rotate: vec![],
        require_signed_skills: false,
        require_signed_packages: false,
        skills_lock_file: None,
        skills_lock_write: false,
        skills_sync: false,
        max_turns: 8,
        request_timeout_ms: 120_000,
        provider_max_retries: 2,
        provider_retry_budget_ms: 0,
        provider_retry_jitter: true,
        turn_timeout_ms: 0,
        json_events: false,
        stream_output: true,
        stream_delay_ms: 0,
        prompt: None,
        orchestrator_mode: CliOrchestratorMode::Off,
        orchestrator_max_plan_steps: 8,
        orchestrator_max_delegated_steps: 8,
        orchestrator_max_executor_response_chars: 20_000,
        orchestrator_max_delegated_step_response_chars: 20_000,
        orchestrator_max_delegated_total_response_chars: 160_000,
        orchestrator_delegate_steps: false,
        prompt_file: None,
        prompt_template_file: None,
        prompt_template_var: vec![],
        command_file: None,
        command_file_error_mode: CliCommandFileErrorMode::FailFast,
        onboard: false,
        onboard_non_interactive: false,
        onboard_profile: "default".to_string(),
        channel_store_root: PathBuf::from(".tau/channel-store"),
        channel_store_inspect: None,
        channel_store_repair: None,
        extension_exec_manifest: None,
        extension_exec_hook: None,
        extension_exec_payload_file: None,
        extension_validate: None,
        extension_list: false,
        extension_list_root: PathBuf::from(".tau/extensions"),
        extension_show: None,
        extension_runtime_hooks: false,
        extension_runtime_root: PathBuf::from(".tau/extensions"),
        package_validate: None,
        package_show: None,
        package_install: None,
        package_install_root: PathBuf::from(".tau/packages"),
        package_update: None,
        package_update_root: PathBuf::from(".tau/packages"),
        package_list: false,
        package_list_root: PathBuf::from(".tau/packages"),
        package_remove: None,
        package_remove_root: PathBuf::from(".tau/packages"),
        package_rollback: None,
        package_rollback_root: PathBuf::from(".tau/packages"),
        package_conflicts: false,
        package_conflicts_root: PathBuf::from(".tau/packages"),
        package_activate: false,
        package_activate_on_startup: false,
        package_activate_root: PathBuf::from(".tau/packages"),
        package_activate_destination: PathBuf::from(".tau/packages-active"),
        package_activate_conflict_policy: "error".to_string(),
        rpc_capabilities: false,
        rpc_validate_frame_file: None,
        rpc_dispatch_frame_file: None,
        rpc_dispatch_ndjson_file: None,
        rpc_serve_ndjson: false,
        events_runner: false,
        events_dir: PathBuf::from(".tau/events"),
        events_state_path: PathBuf::from(".tau/events/state.json"),
        events_poll_interval_ms: 1_000,
        events_queue_limit: 64,
        events_stale_immediate_max_age_seconds: 86_400,
        event_webhook_ingest_file: None,
        event_webhook_channel: None,
        event_webhook_actor_id: None,
        event_webhook_prompt_prefix: "Handle webhook-triggered event.".to_string(),
        event_webhook_debounce_key: None,
        event_webhook_debounce_window_seconds: 60,
        event_webhook_signature: None,
        event_webhook_timestamp: None,
        event_webhook_secret: None,
        event_webhook_secret_id: None,
        event_webhook_signature_algorithm: None,
        event_webhook_signature_max_skew_seconds: 300,
        github_issues_bridge: false,
        github_repo: None,
        github_token: None,
        github_token_id: None,
        github_bot_login: None,
        github_api_base: "https://api.github.com".to_string(),
        github_state_dir: PathBuf::from(".tau/github-issues"),
        github_poll_interval_seconds: 30,
        github_artifact_retention_days: 30,
        github_include_issue_body: false,
        github_include_edited_comments: true,
        github_processed_event_cap: 10_000,
        github_retry_max_attempts: 4,
        github_retry_base_delay_ms: 500,
        slack_bridge: false,
        slack_app_token: None,
        slack_app_token_id: None,
        slack_bot_token: None,
        slack_bot_token_id: None,
        slack_bot_user_id: None,
        slack_api_base: "https://slack.com/api".to_string(),
        slack_state_dir: PathBuf::from(".tau/slack"),
        slack_artifact_retention_days: 30,
        slack_thread_detail_output: true,
        slack_thread_detail_threshold_chars: 1500,
        slack_processed_event_cap: 10_000,
        slack_max_event_age_seconds: 7_200,
        slack_reconnect_delay_ms: 1_000,
        slack_retry_max_attempts: 4,
        slack_retry_base_delay_ms: 500,
        session: PathBuf::from(".tau/sessions/default.jsonl"),
        no_session: false,
        session_validate: false,
        session_import_mode: CliSessionImportMode::Merge,
        branch_from: None,
        session_lock_wait_ms: 5_000,
        session_lock_stale_ms: 30_000,
        allow_path: vec![],
        bash_timeout_ms: 500,
        max_tool_output_bytes: 1024,
        max_file_read_bytes: 2048,
        max_file_write_bytes: 2048,
        max_command_length: 4096,
        allow_command_newlines: true,
        bash_profile: CliBashProfile::Balanced,
        tool_policy_preset: CliToolPolicyPreset::Balanced,
        bash_dry_run: false,
        tool_policy_trace: false,
        allow_command: vec![],
        print_tool_policy: false,
        tool_audit_log: None,
        telemetry_log: None,
        os_sandbox_mode: CliOsSandboxMode::Off,
        os_sandbox_command: vec![],
        enforce_regular_files: true,
    }
}

fn set_workspace_tau_paths(cli: &mut Cli, workspace: &Path) {
    let tau_root = workspace.join(".tau");
    cli.session = tau_root.join("sessions/default.jsonl");
    cli.credential_store = tau_root.join("credentials.json");
    cli.skills_dir = tau_root.join("skills");
    cli.model_catalog_cache = tau_root.join("models/catalog.json");
    cli.channel_store_root = tau_root.join("channel-store");
    cli.events_dir = tau_root.join("events");
    cli.events_state_path = tau_root.join("events/state.json");
    cli.github_state_dir = tau_root.join("github-issues");
    cli.slack_state_dir = tau_root.join("slack");
    cli.package_install_root = tau_root.join("packages");
    cli.package_update_root = tau_root.join("packages");
    cli.package_list_root = tau_root.join("packages");
    cli.package_remove_root = tau_root.join("packages");
    cli.package_rollback_root = tau_root.join("packages");
    cli.package_conflicts_root = tau_root.join("packages");
    cli.package_activate_root = tau_root.join("packages");
    cli.package_activate_destination = tau_root.join("packages-active");
    cli.extension_list_root = tau_root.join("extensions");
    cli.extension_runtime_root = tau_root.join("extensions");
}

fn write_test_provider_credential(
    path: &Path,
    encryption: CredentialStoreEncryptionMode,
    key: Option<&str>,
    provider: Provider,
    record: ProviderCredentialStoreRecord,
) {
    let mut store = CredentialStoreData {
        encryption,
        providers: BTreeMap::new(),
        integrations: BTreeMap::new(),
    };
    store
        .providers
        .insert(provider.as_str().to_string(), record);
    save_credential_store(path, &store, key).expect("save credential store");
}

fn write_test_integration_credential(
    path: &Path,
    encryption: CredentialStoreEncryptionMode,
    key: Option<&str>,
    integration_id: &str,
    record: IntegrationCredentialStoreRecord,
) {
    let mut store = CredentialStoreData {
        encryption,
        providers: BTreeMap::new(),
        integrations: BTreeMap::new(),
    };
    store
        .integrations
        .insert(integration_id.to_string(), record);
    save_credential_store(path, &store, key).expect("save credential store");
}

fn skills_command_config(
    skills_dir: &Path,
    lock_path: &Path,
    trust_root_path: Option<&Path>,
) -> SkillsSyncCommandConfig {
    SkillsSyncCommandConfig {
        skills_dir: skills_dir.to_path_buf(),
        default_lock_path: lock_path.to_path_buf(),
        default_trust_root_path: trust_root_path.map(Path::to_path_buf),
        doctor_config: DoctorCommandConfig {
            model: "openai/gpt-4o-mini".to_string(),
            provider_keys: vec![DoctorProviderKeyStatus {
                provider_kind: Provider::OpenAi,
                provider: "openai".to_string(),
                key_env_var: "OPENAI_API_KEY".to_string(),
                present: true,
                auth_mode: ProviderAuthMethod::ApiKey,
                mode_supported: true,
                login_backend_enabled: false,
                login_backend_executable: None,
                login_backend_available: false,
            }],
            session_enabled: true,
            session_path: PathBuf::from(".tau/sessions/default.jsonl"),
            skills_dir: skills_dir.to_path_buf(),
            skills_lock_path: lock_path.to_path_buf(),
            trust_root_path: trust_root_path.map(Path::to_path_buf),
        },
    }
}

fn test_profile_defaults() -> ProfileDefaults {
    build_profile_defaults(&test_cli())
}

fn test_auth_command_config() -> AuthCommandConfig {
    let mut config = build_auth_command_config(&test_cli());
    if let Ok(current_exe) = std::env::current_exe() {
        config.openai_codex_cli = current_exe.display().to_string();
        config.anthropic_claude_cli = current_exe.display().to_string();
        config.google_gemini_cli = current_exe.display().to_string();
        config.google_gcloud_cli = current_exe.display().to_string();
    }
    config
}

fn test_command_context<'a>(
    tool_policy_json: &'a serde_json::Value,
    profile_defaults: &'a ProfileDefaults,
    skills_command_config: &'a SkillsSyncCommandConfig,
    auth_command_config: &'a AuthCommandConfig,
    model_catalog: &'a ModelCatalog,
) -> CommandExecutionContext<'a> {
    CommandExecutionContext {
        tool_policy_json,
        session_import_mode: SessionImportMode::Merge,
        profile_defaults,
        skills_command_config,
        auth_command_config,
        model_catalog,
        extension_commands: &[],
    }
}

fn set_provider_auth_mode(
    config: &mut AuthCommandConfig,
    provider: Provider,
    mode: ProviderAuthMethod,
) {
    match provider {
        Provider::OpenAi => config.openai_auth_mode = mode,
        Provider::Anthropic => config.anthropic_auth_mode = mode,
        Provider::Google => config.google_auth_mode = mode,
    }
}

fn set_provider_api_key(config: &mut AuthCommandConfig, provider: Provider, value: &str) {
    match provider {
        Provider::OpenAi => config.openai_api_key = Some(value.to_string()),
        Provider::Anthropic => config.anthropic_api_key = Some(value.to_string()),
        Provider::Google => config.google_api_key = Some(value.to_string()),
    }
}

#[test]
fn resolve_api_key_uses_first_non_empty_candidate() {
    let key = resolve_api_key(vec![
        Some("".to_string()),
        Some("  ".to_string()),
        Some("abc".to_string()),
        Some("def".to_string()),
    ]);

    assert_eq!(key, Some("abc".to_string()));
}

#[test]
fn unit_cli_provider_retry_flags_accept_explicit_baseline_values() {
    let cli = Cli::parse_from([
        "tau-rs",
        "--provider-max-retries",
        "2",
        "--provider-retry-budget-ms",
        "0",
        "--provider-retry-jitter",
        "true",
    ]);
    assert_eq!(cli.provider_max_retries, 2);
    assert_eq!(cli.provider_retry_budget_ms, 0);
    assert!(cli.provider_retry_jitter);
}

#[test]
fn functional_cli_provider_retry_flags_accept_overrides() {
    let cli = Cli::parse_from([
        "tau-rs",
        "--provider-max-retries",
        "5",
        "--provider-retry-budget-ms",
        "1500",
        "--provider-retry-jitter",
        "false",
    ]);
    assert_eq!(cli.provider_max_retries, 5);
    assert_eq!(cli.provider_retry_budget_ms, 1500);
    assert!(!cli.provider_retry_jitter);
}

#[test]
fn unit_cli_model_catalog_flags_default_values_are_stable() {
    let cli = Cli::parse_from(["tau-rs"]);
    assert_eq!(cli.model_catalog_url, None);
    assert_eq!(
        cli.model_catalog_cache,
        PathBuf::from(".tau/models/catalog.json")
    );
    assert!(!cli.model_catalog_offline);
    assert_eq!(cli.model_catalog_stale_after_hours, 24);
}

#[test]
fn functional_cli_model_catalog_flags_accept_overrides() {
    let cli = Cli::parse_from([
        "tau-rs",
        "--model-catalog-url",
        "https://example.com/models.json",
        "--model-catalog-cache",
        "/tmp/catalog.json",
        "--model-catalog-offline=true",
        "--model-catalog-stale-after-hours",
        "48",
    ]);
    assert_eq!(
        cli.model_catalog_url.as_deref(),
        Some("https://example.com/models.json")
    );
    assert_eq!(cli.model_catalog_cache, PathBuf::from("/tmp/catalog.json"));
    assert!(cli.model_catalog_offline);
    assert_eq!(cli.model_catalog_stale_after_hours, 48);
}

#[test]
fn unit_cli_extension_validate_flag_defaults_to_none() {
    let cli = Cli::parse_from(["tau-rs"]);
    assert!(cli.extension_exec_manifest.is_none());
    assert!(cli.extension_exec_hook.is_none());
    assert!(cli.extension_exec_payload_file.is_none());
    assert!(cli.extension_validate.is_none());
    assert!(!cli.extension_list);
    assert_eq!(cli.extension_list_root, PathBuf::from(".tau/extensions"));
    assert!(cli.extension_show.is_none());
    assert!(!cli.extension_runtime_hooks);
    assert_eq!(cli.extension_runtime_root, PathBuf::from(".tau/extensions"));
}

#[test]
fn functional_cli_extension_exec_flags_accept_valid_combo() {
    let cli = Cli::parse_from([
        "tau-rs",
        "--extension-exec-manifest",
        "extensions/issue.json",
        "--extension-exec-hook",
        "run-start",
        "--extension-exec-payload-file",
        "extensions/payload.json",
    ]);
    assert_eq!(
        cli.extension_exec_manifest,
        Some(PathBuf::from("extensions/issue.json"))
    );
    assert_eq!(cli.extension_exec_hook.as_deref(), Some("run-start"));
    assert_eq!(
        cli.extension_exec_payload_file,
        Some(PathBuf::from("extensions/payload.json"))
    );
}

#[test]
fn functional_cli_extension_validate_flag_accepts_path() {
    let cli = Cli::parse_from(["tau-rs", "--extension-validate", "extensions/issue.json"]);
    assert_eq!(
        cli.extension_validate,
        Some(PathBuf::from("extensions/issue.json"))
    );
}

#[test]
fn functional_cli_extension_show_flag_accepts_path() {
    let cli = Cli::parse_from(["tau-rs", "--extension-show", "extensions/issue.json"]);
    assert_eq!(
        cli.extension_show,
        Some(PathBuf::from("extensions/issue.json"))
    );
}

#[test]
fn functional_cli_extension_list_flag_accepts_root_override() {
    let cli = Cli::parse_from([
        "tau-rs",
        "--extension-list",
        "--extension-list-root",
        "extensions",
    ]);
    assert!(cli.extension_list);
    assert_eq!(cli.extension_list_root, PathBuf::from("extensions"));
}

#[test]
fn functional_cli_extension_runtime_hook_flags_accept_root_override() {
    let cli = Cli::parse_from([
        "tau-rs",
        "--extension-runtime-hooks",
        "--extension-runtime-root",
        "extensions",
    ]);
    assert!(cli.extension_runtime_hooks);
    assert_eq!(cli.extension_runtime_root, PathBuf::from("extensions"));
}

#[test]
fn regression_cli_extension_show_and_validate_conflict() {
    let parse = Cli::try_parse_from([
        "tau-rs",
        "--extension-show",
        "extensions/issue.json",
        "--extension-validate",
        "extensions/issue.json",
    ]);
    let error = parse.expect_err("show and validate should conflict");
    assert!(error.to_string().contains("cannot be used with"));
}

#[test]
fn regression_cli_extension_list_and_show_conflict() {
    let parse = Cli::try_parse_from([
        "tau-rs",
        "--extension-list",
        "--extension-show",
        "extensions/issue.json",
    ]);
    let error = parse.expect_err("list and show should conflict");
    assert!(error.to_string().contains("cannot be used with"));
}

#[test]
fn regression_cli_extension_exec_requires_hook_and_payload() {
    let parse = Cli::try_parse_from([
        "tau-rs",
        "--extension-exec-manifest",
        "extensions/issue.json",
    ]);
    let error = parse.expect_err("exec manifest should require hook and payload");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn regression_cli_extension_runtime_root_requires_runtime_hooks_flag() {
    let parse = Cli::try_parse_from(["tau-rs", "--extension-runtime-root", "extensions"]);
    let error = parse.expect_err("runtime root should require runtime hooks flag");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn unit_cli_orchestrator_flags_default_values_are_stable() {
    let cli = Cli::parse_from(["tau-rs"]);
    assert_eq!(cli.orchestrator_mode, CliOrchestratorMode::Off);
    assert_eq!(cli.orchestrator_max_plan_steps, 8);
    assert_eq!(cli.orchestrator_max_delegated_steps, 8);
    assert_eq!(cli.orchestrator_max_executor_response_chars, 20_000);
    assert_eq!(cli.orchestrator_max_delegated_step_response_chars, 20_000);
    assert_eq!(cli.orchestrator_max_delegated_total_response_chars, 160_000);
    assert!(!cli.orchestrator_delegate_steps);
}

#[test]
fn functional_cli_orchestrator_flags_accept_overrides() {
    let cli = Cli::parse_from([
        "tau-rs",
        "--orchestrator-mode",
        "plan-first",
        "--orchestrator-max-plan-steps",
        "5",
        "--orchestrator-max-delegated-steps",
        "3",
        "--orchestrator-max-executor-response-chars",
        "160",
        "--orchestrator-max-delegated-step-response-chars",
        "80",
        "--orchestrator-max-delegated-total-response-chars",
        "240",
        "--orchestrator-delegate-steps",
    ]);
    assert_eq!(cli.orchestrator_mode, CliOrchestratorMode::PlanFirst);
    assert_eq!(cli.orchestrator_max_plan_steps, 5);
    assert_eq!(cli.orchestrator_max_delegated_steps, 3);
    assert_eq!(cli.orchestrator_max_executor_response_chars, 160);
    assert_eq!(cli.orchestrator_max_delegated_step_response_chars, 80);
    assert_eq!(cli.orchestrator_max_delegated_total_response_chars, 240);
    assert!(cli.orchestrator_delegate_steps);
}

#[test]
fn regression_cli_orchestrator_executor_response_budget_rejects_zero() {
    let parse = Cli::try_parse_from(["tau-rs", "--orchestrator-max-executor-response-chars", "0"]);
    let error = parse.expect_err("zero executor budget should be rejected");
    assert!(error.to_string().contains("greater than 0"));
}

#[test]
fn regression_cli_orchestrator_delegated_step_count_budget_rejects_zero() {
    let parse = Cli::try_parse_from(["tau-rs", "--orchestrator-max-delegated-steps", "0"]);
    let error = parse.expect_err("zero delegated step count budget should be rejected");
    assert!(error.to_string().contains("greater than 0"));
}

#[test]
fn regression_cli_orchestrator_delegated_step_response_budget_rejects_zero() {
    let parse = Cli::try_parse_from([
        "tau-rs",
        "--orchestrator-max-delegated-step-response-chars",
        "0",
    ]);
    let error = parse.expect_err("zero delegated step budget should be rejected");
    assert!(error.to_string().contains("greater than 0"));
}

#[test]
fn regression_cli_orchestrator_delegated_total_response_budget_rejects_zero() {
    let parse = Cli::try_parse_from([
        "tau-rs",
        "--orchestrator-max-delegated-total-response-chars",
        "0",
    ]);
    let error = parse.expect_err("zero delegated total budget should be rejected");
    assert!(error.to_string().contains("greater than 0"));
}

#[test]
fn unit_cli_provider_auth_mode_flags_default_to_api_key() {
    let cli = Cli::parse_from(["tau-rs"]);
    assert_eq!(cli.openai_auth_mode, CliProviderAuthMode::ApiKey);
    assert_eq!(cli.anthropic_auth_mode, CliProviderAuthMode::ApiKey);
    assert_eq!(cli.google_auth_mode, CliProviderAuthMode::ApiKey);
}

#[test]
fn functional_cli_provider_auth_mode_flags_accept_overrides() {
    let cli = Cli::parse_from([
        "tau-rs",
        "--openai-auth-mode",
        "oauth-token",
        "--anthropic-auth-mode",
        "session-token",
        "--google-auth-mode",
        "adc",
    ]);
    assert_eq!(cli.openai_auth_mode, CliProviderAuthMode::OauthToken);
    assert_eq!(cli.anthropic_auth_mode, CliProviderAuthMode::SessionToken);
    assert_eq!(cli.google_auth_mode, CliProviderAuthMode::Adc);
}

#[test]
fn unit_cli_openai_codex_backend_flags_default_to_enabled() {
    let cli = Cli::parse_from(["tau-rs"]);
    assert!(cli.openai_codex_backend);
    assert_eq!(cli.openai_codex_cli, "codex");
    assert!(cli.openai_codex_args.is_empty());
    assert_eq!(cli.openai_codex_timeout_ms, 120_000);
}

#[test]
fn functional_cli_openai_codex_backend_flags_accept_overrides() {
    let cli = Cli::parse_from([
        "tau-rs",
        "--openai-codex-backend=false",
        "--openai-codex-cli",
        "/tmp/mock-codex",
        "--openai-codex-args=--json,--profile,test",
        "--openai-codex-timeout-ms",
        "9000",
    ]);
    assert!(!cli.openai_codex_backend);
    assert_eq!(cli.openai_codex_cli, "/tmp/mock-codex");
    assert_eq!(
        cli.openai_codex_args,
        vec![
            "--json".to_string(),
            "--profile".to_string(),
            "test".to_string()
        ]
    );
    assert_eq!(cli.openai_codex_timeout_ms, 9000);
}

#[test]
fn unit_cli_anthropic_claude_backend_flags_default_to_enabled() {
    let cli = Cli::parse_from(["tau-rs"]);
    assert!(cli.anthropic_claude_backend);
    assert_eq!(cli.anthropic_claude_cli, "claude");
    assert!(cli.anthropic_claude_args.is_empty());
    assert_eq!(cli.anthropic_claude_timeout_ms, 120_000);
}

#[test]
fn functional_cli_anthropic_claude_backend_flags_accept_overrides() {
    let cli = Cli::parse_from([
        "tau-rs",
        "--anthropic-claude-backend=false",
        "--anthropic-claude-cli",
        "/tmp/mock-claude",
        "--anthropic-claude-args=--print,--verbose",
        "--anthropic-claude-timeout-ms",
        "8000",
    ]);
    assert!(!cli.anthropic_claude_backend);
    assert_eq!(cli.anthropic_claude_cli, "/tmp/mock-claude");
    assert_eq!(
        cli.anthropic_claude_args,
        vec!["--print".to_string(), "--verbose".to_string()]
    );
    assert_eq!(cli.anthropic_claude_timeout_ms, 8000);
}

#[test]
fn unit_cli_google_gemini_backend_flags_default_to_enabled() {
    let cli = Cli::parse_from(["tau-rs"]);
    assert!(cli.google_gemini_backend);
    assert_eq!(cli.google_gemini_cli, "gemini");
    assert_eq!(cli.google_gcloud_cli, "gcloud");
    assert!(cli.google_gemini_args.is_empty());
    assert_eq!(cli.google_gemini_timeout_ms, 120_000);
}

#[test]
fn functional_cli_google_gemini_backend_flags_accept_overrides() {
    let cli = Cli::parse_from([
        "tau-rs",
        "--google-gemini-backend=false",
        "--google-gemini-cli",
        "/tmp/mock-gemini",
        "--google-gcloud-cli",
        "/tmp/mock-gcloud",
        "--google-gemini-args=--sandbox,readonly,--profile,test",
        "--google-gemini-timeout-ms",
        "7000",
    ]);
    assert!(!cli.google_gemini_backend);
    assert_eq!(cli.google_gemini_cli, "/tmp/mock-gemini");
    assert_eq!(cli.google_gcloud_cli, "/tmp/mock-gcloud");
    assert_eq!(
        cli.google_gemini_args,
        vec![
            "--sandbox".to_string(),
            "readonly".to_string(),
            "--profile".to_string(),
            "test".to_string()
        ]
    );
    assert_eq!(cli.google_gemini_timeout_ms, 7000);
}

#[test]
fn unit_cli_credential_store_flags_default_to_auto_mode_and_default_path() {
    let cli = Cli::parse_from(["tau-rs"]);
    assert_eq!(cli.credential_store, PathBuf::from(".tau/credentials.json"));
    assert!(cli.credential_store_key.is_none());
    assert_eq!(
        cli.credential_store_encryption,
        CliCredentialStoreEncryptionMode::Auto
    );
}

#[test]
fn functional_cli_credential_store_flags_accept_explicit_overrides() {
    let cli = Cli::parse_from([
        "tau-rs",
        "--credential-store",
        "custom/credentials.json",
        "--credential-store-key",
        "secret-store-key",
        "--credential-store-encryption",
        "keyed",
    ]);
    assert_eq!(
        cli.credential_store,
        PathBuf::from("custom/credentials.json")
    );
    assert_eq!(
        cli.credential_store_key.as_deref(),
        Some("secret-store-key")
    );
    assert_eq!(
        cli.credential_store_encryption,
        CliCredentialStoreEncryptionMode::Keyed
    );
}

#[test]
fn unit_cli_integration_secret_id_flags_default_to_none() {
    let cli = Cli::parse_from(["tau-rs"]);
    assert!(cli.event_webhook_secret_id.is_none());
    assert!(cli.github_token_id.is_none());
    assert!(cli.slack_app_token_id.is_none());
    assert!(cli.slack_bot_token_id.is_none());
}

#[test]
fn functional_cli_integration_secret_id_flags_accept_explicit_values() {
    let cli = Cli::parse_from([
        "tau-rs",
        "--event-webhook-ingest-file",
        "payload.json",
        "--github-issues-bridge",
        "--slack-bridge",
        "--event-webhook-secret-id",
        "event-webhook-secret",
        "--github-token-id",
        "github-token",
        "--slack-app-token-id",
        "slack-app-token",
        "--slack-bot-token-id",
        "slack-bot-token",
    ]);
    assert_eq!(
        cli.event_webhook_secret_id.as_deref(),
        Some("event-webhook-secret")
    );
    assert_eq!(cli.github_token_id.as_deref(), Some("github-token"));
    assert_eq!(cli.slack_app_token_id.as_deref(), Some("slack-app-token"));
    assert_eq!(cli.slack_bot_token_id.as_deref(), Some("slack-bot-token"));
}

#[test]
fn unit_cli_artifact_retention_flags_default_to_30_days() {
    let cli = Cli::parse_from(["tau-rs"]);
    assert_eq!(cli.github_artifact_retention_days, 30);
    assert_eq!(cli.slack_artifact_retention_days, 30);
}

#[test]
fn functional_cli_artifact_retention_flags_accept_explicit_values() {
    let cli = Cli::parse_from([
        "tau-rs",
        "--github-issues-bridge",
        "--slack-bridge",
        "--github-artifact-retention-days",
        "14",
        "--slack-artifact-retention-days",
        "0",
    ]);
    assert_eq!(cli.github_artifact_retention_days, 14);
    assert_eq!(cli.slack_artifact_retention_days, 0);
}

#[test]
fn unit_cli_rpc_flags_default_to_disabled() {
    let cli = Cli::parse_from(["tau-rs"]);
    assert!(!cli.rpc_capabilities);
    assert!(cli.rpc_validate_frame_file.is_none());
    assert!(cli.rpc_dispatch_frame_file.is_none());
    assert!(cli.rpc_dispatch_ndjson_file.is_none());
    assert!(!cli.rpc_serve_ndjson);
}

#[test]
fn functional_cli_rpc_serve_ndjson_flag_accepts_enablement() {
    let cli = Cli::parse_from(["tau-rs", "--rpc-serve-ndjson"]);
    assert!(cli.rpc_serve_ndjson);
}

#[test]
fn regression_cli_rpc_serve_ndjson_conflicts_with_rpc_capabilities() {
    let parse = Cli::try_parse_from(["tau-rs", "--rpc-serve-ndjson", "--rpc-capabilities"]);
    let error = parse.expect_err("rpc serve ndjson and rpc capabilities should conflict");
    assert!(error.to_string().contains("cannot be used with"));
}

#[test]
fn regression_cli_rpc_serve_ndjson_conflicts_with_rpc_dispatch_ndjson_file() {
    let parse = Cli::try_parse_from([
        "tau-rs",
        "--rpc-serve-ndjson",
        "--rpc-dispatch-ndjson-file",
        "fixtures/rpc.ndjson",
    ]);
    let error = parse.expect_err("rpc serve ndjson and rpc dispatch ndjson should conflict");
    assert!(error.to_string().contains("cannot be used with"));
}

#[test]
fn unit_parse_auth_command_supports_login_status_logout_and_json() {
    let login = parse_auth_command("login openai --mode oauth-token --launch --json")
        .expect("parse auth login");
    assert_eq!(
        login,
        AuthCommand::Login {
            provider: Provider::OpenAi,
            mode: Some(ProviderAuthMethod::OauthToken),
            launch: true,
            json_output: true,
        }
    );

    let status = parse_auth_command("status anthropic --json").expect("parse auth status");
    assert_eq!(
        status,
        AuthCommand::Status {
            provider: Some(Provider::Anthropic),
            mode: None,
            mode_support: AuthMatrixModeSupportFilter::All,
            availability: AuthMatrixAvailabilityFilter::All,
            state: None,
            source_kind: AuthSourceKindFilter::All,
            revoked: AuthRevokedFilter::All,
            json_output: true,
        }
    );

    let filtered_status =
        parse_auth_command("status --availability unavailable openai --state Ready --json")
            .expect("parse filtered auth status");
    assert_eq!(
        filtered_status,
        AuthCommand::Status {
            provider: Some(Provider::OpenAi),
            mode: None,
            mode_support: AuthMatrixModeSupportFilter::All,
            availability: AuthMatrixAvailabilityFilter::Unavailable,
            state: Some("ready".to_string()),
            source_kind: AuthSourceKindFilter::All,
            revoked: AuthRevokedFilter::All,
            json_output: true,
        }
    );

    let mode_filtered_status = parse_auth_command("status openai --mode session-token --json")
        .expect("parse mode-filtered auth status");
    assert_eq!(
        mode_filtered_status,
        AuthCommand::Status {
            provider: Some(Provider::OpenAi),
            mode: Some(ProviderAuthMethod::SessionToken),
            mode_support: AuthMatrixModeSupportFilter::All,
            availability: AuthMatrixAvailabilityFilter::All,
            state: None,
            source_kind: AuthSourceKindFilter::All,
            revoked: AuthRevokedFilter::All,
            json_output: true,
        }
    );

    let supported_status = parse_auth_command("status --mode-support supported --json")
        .expect("parse mode-support filtered auth status");
    assert_eq!(
        supported_status,
        AuthCommand::Status {
            provider: None,
            mode: None,
            mode_support: AuthMatrixModeSupportFilter::Supported,
            availability: AuthMatrixAvailabilityFilter::All,
            state: None,
            source_kind: AuthSourceKindFilter::All,
            revoked: AuthRevokedFilter::All,
            json_output: true,
        }
    );

    let source_kind_filtered_status = parse_auth_command("status --source-kind env --json")
        .expect("parse source-kind filtered auth status");
    assert_eq!(
        source_kind_filtered_status,
        AuthCommand::Status {
            provider: None,
            mode: None,
            mode_support: AuthMatrixModeSupportFilter::All,
            availability: AuthMatrixAvailabilityFilter::All,
            state: None,
            source_kind: AuthSourceKindFilter::Env,
            revoked: AuthRevokedFilter::All,
            json_output: true,
        }
    );

    let revoked_filtered_status = parse_auth_command("status --revoked revoked --json")
        .expect("parse revoked filtered auth status");
    assert_eq!(
        revoked_filtered_status,
        AuthCommand::Status {
            provider: None,
            mode: None,
            mode_support: AuthMatrixModeSupportFilter::All,
            availability: AuthMatrixAvailabilityFilter::All,
            state: None,
            source_kind: AuthSourceKindFilter::All,
            revoked: AuthRevokedFilter::Revoked,
            json_output: true,
        }
    );

    let logout = parse_auth_command("logout google").expect("parse auth logout");
    assert_eq!(
        logout,
        AuthCommand::Logout {
            provider: Provider::Google,
            json_output: false,
        }
    );

    let openrouter_login =
        parse_auth_command("login openrouter --mode api-key").expect("parse openrouter login");
    assert_eq!(
        openrouter_login,
        AuthCommand::Login {
            provider: Provider::OpenAi,
            mode: Some(ProviderAuthMethod::ApiKey),
            launch: false,
            json_output: false,
        }
    );

    let groq_login = parse_auth_command("login groq --mode api-key").expect("parse groq login");
    assert_eq!(
        groq_login,
        AuthCommand::Login {
            provider: Provider::OpenAi,
            mode: Some(ProviderAuthMethod::ApiKey),
            launch: false,
            json_output: false,
        }
    );

    let xai_login = parse_auth_command("login xai --mode api-key").expect("parse xai login");
    assert_eq!(
        xai_login,
        AuthCommand::Login {
            provider: Provider::OpenAi,
            mode: Some(ProviderAuthMethod::ApiKey),
            launch: false,
            json_output: false,
        }
    );

    let mistral_login =
        parse_auth_command("login mistral --mode api-key").expect("parse mistral login");
    assert_eq!(
        mistral_login,
        AuthCommand::Login {
            provider: Provider::OpenAi,
            mode: Some(ProviderAuthMethod::ApiKey),
            launch: false,
            json_output: false,
        }
    );

    let azure_login = parse_auth_command("login azure --mode api-key").expect("parse azure login");
    assert_eq!(
        azure_login,
        AuthCommand::Login {
            provider: Provider::OpenAi,
            mode: Some(ProviderAuthMethod::ApiKey),
            launch: false,
            json_output: false,
        }
    );

    let matrix = parse_auth_command("matrix --json").expect("parse auth matrix");
    assert_eq!(
        matrix,
        AuthCommand::Matrix {
            provider: None,
            mode: None,
            mode_support: AuthMatrixModeSupportFilter::All,
            availability: AuthMatrixAvailabilityFilter::All,
            state: None,
            source_kind: AuthSourceKindFilter::All,
            revoked: AuthRevokedFilter::All,
            json_output: true,
        }
    );

    let filtered_matrix = parse_auth_command("matrix openai --mode oauth-token --json")
        .expect("parse filtered auth matrix");
    assert_eq!(
        filtered_matrix,
        AuthCommand::Matrix {
            provider: Some(Provider::OpenAi),
            mode: Some(ProviderAuthMethod::OauthToken),
            mode_support: AuthMatrixModeSupportFilter::All,
            availability: AuthMatrixAvailabilityFilter::All,
            state: None,
            source_kind: AuthSourceKindFilter::All,
            revoked: AuthRevokedFilter::All,
            json_output: true,
        }
    );

    let available_only_matrix = parse_auth_command("matrix --availability available --json")
        .expect("parse available-only auth matrix");
    assert_eq!(
        available_only_matrix,
        AuthCommand::Matrix {
            provider: None,
            mode: None,
            mode_support: AuthMatrixModeSupportFilter::All,
            availability: AuthMatrixAvailabilityFilter::Available,
            state: None,
            source_kind: AuthSourceKindFilter::All,
            revoked: AuthRevokedFilter::All,
            json_output: true,
        }
    );

    let state_filtered_matrix = parse_auth_command("matrix --state ready --json")
        .expect("parse state-filtered auth matrix");
    assert_eq!(
        state_filtered_matrix,
        AuthCommand::Matrix {
            provider: None,
            mode: None,
            mode_support: AuthMatrixModeSupportFilter::All,
            availability: AuthMatrixAvailabilityFilter::All,
            state: Some("ready".to_string()),
            source_kind: AuthSourceKindFilter::All,
            revoked: AuthRevokedFilter::All,
            json_output: true,
        }
    );

    let supported_only_matrix = parse_auth_command("matrix --mode-support supported --json")
        .expect("parse supported-only auth matrix");
    assert_eq!(
        supported_only_matrix,
        AuthCommand::Matrix {
            provider: None,
            mode: None,
            mode_support: AuthMatrixModeSupportFilter::Supported,
            availability: AuthMatrixAvailabilityFilter::All,
            state: None,
            source_kind: AuthSourceKindFilter::All,
            revoked: AuthRevokedFilter::All,
            json_output: true,
        }
    );

    let source_kind_filtered_matrix =
        parse_auth_command("matrix --source-kind credential-store --json")
            .expect("parse source-kind filtered auth matrix");
    assert_eq!(
        source_kind_filtered_matrix,
        AuthCommand::Matrix {
            provider: None,
            mode: None,
            mode_support: AuthMatrixModeSupportFilter::All,
            availability: AuthMatrixAvailabilityFilter::All,
            state: None,
            source_kind: AuthSourceKindFilter::CredentialStore,
            revoked: AuthRevokedFilter::All,
            json_output: true,
        }
    );

    let revoked_filtered_matrix = parse_auth_command("matrix --revoked not-revoked --json")
        .expect("parse revoked filtered auth matrix");
    assert_eq!(
        revoked_filtered_matrix,
        AuthCommand::Matrix {
            provider: None,
            mode: None,
            mode_support: AuthMatrixModeSupportFilter::All,
            availability: AuthMatrixAvailabilityFilter::All,
            state: None,
            source_kind: AuthSourceKindFilter::All,
            revoked: AuthRevokedFilter::NotRevoked,
            json_output: true,
        }
    );
}

#[test]
fn regression_parse_auth_command_rejects_unknown_provider_mode_and_usage_errors() {
    let unknown_provider =
        parse_auth_command("login mystery --mode oauth-token").expect_err("provider fail");
    assert!(unknown_provider.to_string().contains("unknown provider"));

    let unknown_mode = parse_auth_command("login openai --mode unknown").expect_err("mode fail");
    assert!(unknown_mode.to_string().contains("unknown auth mode"));

    let missing_login_provider = parse_auth_command("login").expect_err("usage fail for login");
    assert!(missing_login_provider
        .to_string()
        .contains("usage: /auth login"));

    let duplicate_login_launch =
        parse_auth_command("login openai --launch --launch").expect_err("duplicate launch flag");
    assert!(duplicate_login_launch
        .to_string()
        .contains("usage: /auth login"));

    let invalid_matrix_args =
        parse_auth_command("matrix openai anthropic").expect_err("matrix args fail");
    assert!(invalid_matrix_args
        .to_string()
        .contains("usage: /auth matrix"));

    let missing_matrix_mode = parse_auth_command("matrix --mode").expect_err("missing matrix mode");
    assert!(missing_matrix_mode
        .to_string()
        .contains("usage: /auth matrix"));

    let missing_matrix_mode_support =
        parse_auth_command("matrix --mode-support").expect_err("missing matrix mode-support");
    assert!(missing_matrix_mode_support
        .to_string()
        .contains("usage: /auth matrix"));

    let duplicate_matrix_mode_support =
        parse_auth_command("matrix --mode-support all --mode-support supported")
            .expect_err("duplicate matrix mode-support");
    assert!(duplicate_matrix_mode_support
        .to_string()
        .contains("usage: /auth matrix"));

    let unknown_matrix_mode_support =
        parse_auth_command("matrix --mode-support maybe").expect_err("unknown matrix mode-support");
    assert!(unknown_matrix_mode_support
        .to_string()
        .contains("unknown mode-support filter"));

    let missing_matrix_availability =
        parse_auth_command("matrix --availability").expect_err("missing matrix availability");
    assert!(missing_matrix_availability
        .to_string()
        .contains("usage: /auth matrix"));

    let missing_status_availability =
        parse_auth_command("status --availability").expect_err("missing status availability");
    assert!(missing_status_availability
        .to_string()
        .contains("usage: /auth status"));

    let missing_status_mode_support =
        parse_auth_command("status --mode-support").expect_err("missing status mode-support");
    assert!(missing_status_mode_support
        .to_string()
        .contains("usage: /auth status"));

    let missing_status_mode = parse_auth_command("status --mode").expect_err("missing status mode");
    assert!(missing_status_mode
        .to_string()
        .contains("usage: /auth status"));

    let duplicate_status_availability =
        parse_auth_command("status --availability all --availability unavailable")
            .expect_err("duplicate status availability");
    assert!(duplicate_status_availability
        .to_string()
        .contains("usage: /auth status"));

    let duplicate_status_mode =
        parse_auth_command("status --mode api-key --mode adc").expect_err("duplicate status mode");
    assert!(duplicate_status_mode
        .to_string()
        .contains("usage: /auth status"));

    let duplicate_status_mode_support =
        parse_auth_command("status --mode-support all --mode-support supported")
            .expect_err("duplicate status mode-support");
    assert!(duplicate_status_mode_support
        .to_string()
        .contains("usage: /auth status"));

    let unknown_matrix_availability = parse_auth_command("matrix --availability sometimes")
        .expect_err("unknown matrix availability");
    assert!(unknown_matrix_availability
        .to_string()
        .contains("unknown availability filter"));

    let unknown_status_availability = parse_auth_command("status --availability sometimes")
        .expect_err("unknown status availability");
    assert!(unknown_status_availability
        .to_string()
        .contains("unknown availability filter"));

    let unknown_status_mode_support =
        parse_auth_command("status --mode-support maybe").expect_err("unknown status mode-support");
    assert!(unknown_status_mode_support
        .to_string()
        .contains("unknown mode-support filter"));

    let unknown_status_mode =
        parse_auth_command("status --mode impossible").expect_err("unknown status mode");
    assert!(unknown_status_mode
        .to_string()
        .contains("unknown auth mode"));

    let missing_matrix_state =
        parse_auth_command("matrix --state").expect_err("missing matrix state filter");
    assert!(missing_matrix_state
        .to_string()
        .contains("usage: /auth matrix"));

    let missing_status_state =
        parse_auth_command("status --state").expect_err("missing status state filter");
    assert!(missing_status_state
        .to_string()
        .contains("usage: /auth status"));

    let duplicate_status_state = parse_auth_command("status --state ready --state revoked")
        .expect_err("duplicate status state filter");
    assert!(duplicate_status_state
        .to_string()
        .contains("usage: /auth status"));

    let duplicate_matrix_state = parse_auth_command("matrix --state ready --state revoked")
        .expect_err("duplicate matrix state filter");
    assert!(duplicate_matrix_state
        .to_string()
        .contains("usage: /auth matrix"));

    let missing_status_source_kind =
        parse_auth_command("status --source-kind").expect_err("missing status source-kind");
    assert!(missing_status_source_kind
        .to_string()
        .contains("usage: /auth status"));

    let duplicate_status_source_kind =
        parse_auth_command("status --source-kind all --source-kind env")
            .expect_err("duplicate status source-kind");
    assert!(duplicate_status_source_kind
        .to_string()
        .contains("usage: /auth status"));

    let unknown_status_source_kind = parse_auth_command("status --source-kind wildcard")
        .expect_err("unknown status source-kind");
    assert!(unknown_status_source_kind
        .to_string()
        .contains("unknown source-kind filter"));

    let missing_matrix_source_kind =
        parse_auth_command("matrix --source-kind").expect_err("missing matrix source-kind");
    assert!(missing_matrix_source_kind
        .to_string()
        .contains("usage: /auth matrix"));

    let duplicate_matrix_source_kind =
        parse_auth_command("matrix --source-kind all --source-kind env")
            .expect_err("duplicate matrix source-kind");
    assert!(duplicate_matrix_source_kind
        .to_string()
        .contains("usage: /auth matrix"));

    let unknown_matrix_source_kind = parse_auth_command("matrix --source-kind wildcard")
        .expect_err("unknown matrix source-kind");
    assert!(unknown_matrix_source_kind
        .to_string()
        .contains("unknown source-kind filter"));

    let missing_status_revoked =
        parse_auth_command("status --revoked").expect_err("missing status revoked filter");
    assert!(missing_status_revoked
        .to_string()
        .contains("usage: /auth status"));

    let duplicate_status_revoked = parse_auth_command("status --revoked all --revoked revoked")
        .expect_err("duplicate status revoked filter");
    assert!(duplicate_status_revoked
        .to_string()
        .contains("usage: /auth status"));

    let unknown_status_revoked =
        parse_auth_command("status --revoked maybe").expect_err("unknown status revoked filter");
    assert!(unknown_status_revoked
        .to_string()
        .contains("unknown revoked filter"));

    let missing_matrix_revoked =
        parse_auth_command("matrix --revoked").expect_err("missing matrix revoked filter");
    assert!(missing_matrix_revoked
        .to_string()
        .contains("usage: /auth matrix"));

    let duplicate_matrix_revoked = parse_auth_command("matrix --revoked all --revoked revoked")
        .expect_err("duplicate matrix revoked filter");
    assert!(duplicate_matrix_revoked
        .to_string()
        .contains("usage: /auth matrix"));

    let unknown_matrix_revoked =
        parse_auth_command("matrix --revoked maybe").expect_err("unknown matrix revoked filter");
    assert!(unknown_matrix_revoked
        .to_string()
        .contains("unknown revoked filter"));

    let unknown_subcommand = parse_auth_command("noop").expect_err("subcommand fail");
    assert!(unknown_subcommand.to_string().contains("usage: /auth"));
}

#[test]
fn unit_parse_integration_auth_command_supports_set_status_rotate_revoke_and_json() {
    let set = parse_integration_auth_command("set github-token ghp_token --json")
        .expect("parse integration set");
    assert_eq!(
        set,
        IntegrationAuthCommand::Set {
            integration_id: "github-token".to_string(),
            secret: "ghp_token".to_string(),
            json_output: true,
        }
    );

    let status = parse_integration_auth_command("status slack-app-token --json")
        .expect("parse integration status");
    assert_eq!(
        status,
        IntegrationAuthCommand::Status {
            integration_id: Some("slack-app-token".to_string()),
            json_output: true,
        }
    );

    let rotate = parse_integration_auth_command("rotate slack-bot-token next_secret")
        .expect("parse integration rotate");
    assert_eq!(
        rotate,
        IntegrationAuthCommand::Rotate {
            integration_id: "slack-bot-token".to_string(),
            secret: "next_secret".to_string(),
            json_output: false,
        }
    );

    let revoke = parse_integration_auth_command("revoke event-webhook-secret")
        .expect("parse integration revoke");
    assert_eq!(
        revoke,
        IntegrationAuthCommand::Revoke {
            integration_id: "event-webhook-secret".to_string(),
            json_output: false,
        }
    );
}

#[test]
fn regression_parse_integration_auth_command_rejects_usage_and_invalid_ids() {
    let error = parse_integration_auth_command("set github-token").expect_err("missing secret");
    assert!(error
        .to_string()
        .contains("usage: /integration-auth set <integration-id> <secret> [--json]"));

    let error = parse_integration_auth_command("status bad$id").expect_err("invalid id");
    assert!(error.to_string().contains("contains unsupported character"));

    let error = parse_integration_auth_command("unknown").expect_err("unknown subcommand");
    assert!(error
        .to_string()
        .contains("usage: /integration-auth <set|status|rotate|revoke> ..."));
}

#[test]
fn unit_auth_conformance_provider_capability_matrix_matches_expected_support() {
    let cases = vec![
        (
            Provider::OpenAi,
            ProviderAuthMethod::ApiKey,
            true,
            "supported",
        ),
        (
            Provider::OpenAi,
            ProviderAuthMethod::OauthToken,
            true,
            "supported",
        ),
        (
            Provider::OpenAi,
            ProviderAuthMethod::SessionToken,
            true,
            "supported",
        ),
        (
            Provider::OpenAi,
            ProviderAuthMethod::Adc,
            false,
            "not_implemented",
        ),
        (
            Provider::Anthropic,
            ProviderAuthMethod::ApiKey,
            true,
            "supported",
        ),
        (
            Provider::Anthropic,
            ProviderAuthMethod::OauthToken,
            true,
            "supported",
        ),
        (
            Provider::Anthropic,
            ProviderAuthMethod::SessionToken,
            true,
            "supported",
        ),
        (
            Provider::Anthropic,
            ProviderAuthMethod::Adc,
            false,
            "not_implemented",
        ),
        (
            Provider::Google,
            ProviderAuthMethod::ApiKey,
            true,
            "supported",
        ),
        (
            Provider::Google,
            ProviderAuthMethod::OauthToken,
            true,
            "supported",
        ),
        (
            Provider::Google,
            ProviderAuthMethod::SessionToken,
            false,
            "unsupported",
        ),
        (Provider::Google, ProviderAuthMethod::Adc, true, "supported"),
    ];

    for (provider, mode, expected_supported, expected_reason) in cases {
        let capability = provider_auth_capability(provider, mode);
        assert_eq!(capability.supported, expected_supported);
        assert_eq!(capability.reason, expected_reason);
    }
}

#[test]
fn unit_auth_state_count_helpers_are_deterministic() {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let snapshot = snapshot_env_vars(&[
        "OPENAI_API_KEY",
        "OPENROUTER_API_KEY",
        "GROQ_API_KEY",
        "XAI_API_KEY",
        "MISTRAL_API_KEY",
        "AZURE_OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "GEMINI_API_KEY",
        "GOOGLE_API_KEY",
        "TAU_API_KEY",
    ]);
    for key in [
        "OPENAI_API_KEY",
        "OPENROUTER_API_KEY",
        "GROQ_API_KEY",
        "XAI_API_KEY",
        "MISTRAL_API_KEY",
        "AZURE_OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "GEMINI_API_KEY",
        "GOOGLE_API_KEY",
        "TAU_API_KEY",
    ] {
        std::env::remove_var(key);
    }

    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-state-helper-counts.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.openai_auth_mode = ProviderAuthMethod::SessionToken;
    config.openai_api_key = None;
    config.anthropic_api_key = None;
    config.google_api_key = None;

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::SessionToken,
            access_token: Some("helper-revoked-access".to_string()),
            refresh_token: Some("helper-revoked-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(300)),
            revoked: true,
        },
    );
    let store = load_credential_store(
        &config.credential_store,
        config.credential_store_encryption,
        config.credential_store_key.as_deref(),
    )
    .expect("load helper credential store");

    let rows = vec![
        auth_status_row_for_provider(&config, Provider::OpenAi, Some(&store), None),
        auth_status_row_for_provider(&config, Provider::Anthropic, None, None),
        auth_status_row_for_provider(&config, Provider::Google, None, None),
    ];
    let mode_counts = auth_mode_counts(&rows);
    assert_eq!(mode_counts.get("session_token"), Some(&1));
    assert_eq!(mode_counts.get("api_key"), Some(&2));
    assert_eq!(
        format_auth_state_counts(&mode_counts),
        "api_key:2,session_token:1"
    );
    let provider_counts = auth_provider_counts(&rows);
    assert_eq!(provider_counts.get("openai"), Some(&1));
    assert_eq!(provider_counts.get("anthropic"), Some(&1));
    assert_eq!(provider_counts.get("google"), Some(&1));
    assert_eq!(
        format_auth_state_counts(&provider_counts),
        "anthropic:1,google:1,openai:1"
    );
    let availability_counts = auth_availability_counts(&rows);
    assert_eq!(availability_counts.get("available"), None);
    assert_eq!(availability_counts.get("unavailable"), Some(&3));
    assert_eq!(
        format_auth_state_counts(&availability_counts),
        "unavailable:3"
    );
    let counts = auth_state_counts(&rows);
    assert_eq!(counts.get("revoked"), Some(&1));
    assert_eq!(counts.get("missing_api_key"), Some(&2));
    let source_kind_counts = auth_source_kind_counts(&rows);
    assert_eq!(source_kind_counts.get("credential_store"), Some(&1));
    assert_eq!(source_kind_counts.get("none"), Some(&2));
    let revoked_counts = auth_revoked_counts(&rows);
    assert_eq!(revoked_counts.get("revoked"), Some(&1));
    assert_eq!(revoked_counts.get("not_revoked"), Some(&2));
    assert_eq!(
        format_auth_state_counts(&revoked_counts),
        "not_revoked:2,revoked:1"
    );
    assert_eq!(
        format_auth_state_counts(&source_kind_counts),
        "credential_store:1,none:2"
    );
    assert_eq!(auth_source_kind("--api-key"), "flag");
    assert_eq!(auth_source_kind("OPENAI_API_KEY"), "env");
    assert_eq!(auth_source_kind("credential_store"), "credential_store");
    assert_eq!(auth_source_kind("none"), "none");

    assert_eq!(
        format_auth_state_counts(&counts),
        "missing_api_key:2,revoked:1"
    );
    assert_eq!(
        format_auth_state_counts(&std::collections::BTreeMap::new()),
        "none"
    );

    restore_env_vars(snapshot);
}

#[test]
fn unit_provider_auth_snapshot_reports_refreshable_and_expiration() {
    let temp = tempdir().expect("tempdir");
    let now = current_unix_timestamp();
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-snapshot-ready.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = None;
    config.openai_api_key = None;
    set_provider_auth_mode(
        &mut config,
        Provider::OpenAi,
        ProviderAuthMethod::OauthToken,
    );

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("oauth-access-snapshot".to_string()),
            refresh_token: Some("oauth-refresh-snapshot".to_string()),
            expires_unix: Some(now.saturating_add(120)),
            revoked: false,
        },
    );
    let store = load_credential_store(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
    )
    .expect("load snapshot store");

    let snapshot = provider_auth_snapshot_for_status(&config, Provider::OpenAi, Some(&store), None);
    assert_eq!(snapshot.method, ProviderAuthMethod::OauthToken);
    assert!(snapshot.available);
    assert_eq!(snapshot.state, "ready");
    assert_eq!(snapshot.source, "credential_store");
    assert_eq!(snapshot.expires_unix, Some(now.saturating_add(120)));
    assert!(snapshot.refreshable);
    assert!(!snapshot.revoked);
    assert_eq!(snapshot.secret.as_deref(), Some("oauth-access-snapshot"));
}

#[test]
fn unit_provider_auth_snapshot_marks_revoked_not_refreshable() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-snapshot-revoked.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = None;
    config.openai_api_key = None;
    set_provider_auth_mode(
        &mut config,
        Provider::OpenAi,
        ProviderAuthMethod::SessionToken,
    );

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::SessionToken,
            access_token: Some("revoked-access".to_string()),
            refresh_token: Some("revoked-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(300)),
            revoked: true,
        },
    );
    let store = load_credential_store(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
    )
    .expect("load revoked snapshot store");

    let snapshot = provider_auth_snapshot_for_status(&config, Provider::OpenAi, Some(&store), None);
    assert!(!snapshot.available);
    assert_eq!(snapshot.state, "revoked");
    assert_eq!(snapshot.source, "credential_store");
    assert!(snapshot.revoked);
    assert!(!snapshot.refreshable);
    assert!(snapshot.secret.is_none());
}

#[test]
fn functional_auth_conformance_status_matrix_reports_expected_rows() {
    #[derive(Debug)]
    struct AuthConformanceCase {
        provider: Provider,
        mode: ProviderAuthMethod,
        api_key: Option<&'static str>,
        store_record: Option<ProviderCredentialStoreRecord>,
        expected_state: &'static str,
        expected_available: bool,
        expected_source: &'static str,
    }

    let temp = tempdir().expect("tempdir");
    let future_expiry = current_unix_timestamp().saturating_add(600);
    let cases = vec![
        AuthConformanceCase {
            provider: Provider::OpenAi,
            mode: ProviderAuthMethod::ApiKey,
            api_key: Some("openai-conformance-key"),
            store_record: None,
            expected_state: "ready",
            expected_available: true,
            expected_source: "--openai-api-key",
        },
        AuthConformanceCase {
            provider: Provider::Anthropic,
            mode: ProviderAuthMethod::ApiKey,
            api_key: Some("anthropic-conformance-key"),
            store_record: None,
            expected_state: "ready",
            expected_available: true,
            expected_source: "--anthropic-api-key",
        },
        AuthConformanceCase {
            provider: Provider::Google,
            mode: ProviderAuthMethod::ApiKey,
            api_key: Some("google-conformance-key"),
            store_record: None,
            expected_state: "ready",
            expected_available: true,
            expected_source: "--google-api-key",
        },
        AuthConformanceCase {
            provider: Provider::OpenAi,
            mode: ProviderAuthMethod::OauthToken,
            api_key: None,
            store_record: Some(ProviderCredentialStoreRecord {
                auth_method: ProviderAuthMethod::OauthToken,
                access_token: Some("oauth-access".to_string()),
                refresh_token: Some("oauth-refresh".to_string()),
                expires_unix: Some(future_expiry),
                revoked: false,
            }),
            expected_state: "ready",
            expected_available: true,
            expected_source: "credential_store",
        },
        AuthConformanceCase {
            provider: Provider::OpenAi,
            mode: ProviderAuthMethod::SessionToken,
            api_key: None,
            store_record: Some(ProviderCredentialStoreRecord {
                auth_method: ProviderAuthMethod::SessionToken,
                access_token: Some("session-access".to_string()),
                refresh_token: Some("session-refresh".to_string()),
                expires_unix: Some(future_expiry),
                revoked: false,
            }),
            expected_state: "ready",
            expected_available: true,
            expected_source: "credential_store",
        },
        AuthConformanceCase {
            provider: Provider::Anthropic,
            mode: ProviderAuthMethod::OauthToken,
            api_key: None,
            store_record: None,
            expected_state: "ready",
            expected_available: true,
            expected_source: "claude_cli",
        },
        AuthConformanceCase {
            provider: Provider::Google,
            mode: ProviderAuthMethod::SessionToken,
            api_key: None,
            store_record: None,
            expected_state: "unsupported_mode",
            expected_available: false,
            expected_source: "none",
        },
    ];

    let mut matrix_rows = Vec::new();
    for (index, case) in cases.into_iter().enumerate() {
        let mut config = test_auth_command_config();
        config.credential_store = temp.path().join(format!("auth-conformance-{index}.json"));
        config.credential_store_encryption = CredentialStoreEncryptionMode::None;
        config.api_key = None;
        config.openai_api_key = None;
        config.anthropic_api_key = None;
        config.google_api_key = None;
        set_provider_auth_mode(&mut config, case.provider, case.mode);
        if let Some(api_key) = case.api_key {
            set_provider_api_key(&mut config, case.provider, api_key);
        }
        if let Some(record) = case.store_record {
            write_test_provider_credential(
                &config.credential_store,
                CredentialStoreEncryptionMode::None,
                None,
                case.provider,
                record,
            );
        }

        let output = execute_auth_command(
            &config,
            &format!("status {} --json", case.provider.as_str()),
        );
        let payload: serde_json::Value = serde_json::from_str(&output).expect("parse status");
        let row = &payload["entries"][0];
        matrix_rows.push(format!(
            "{}:{}:{}:{}",
            case.provider.as_str(),
            case.mode.as_str(),
            row["state"].as_str().unwrap_or("unknown"),
            row["available"].as_bool().unwrap_or(false)
        ));
        assert_eq!(row["provider"], case.provider.as_str());
        assert_eq!(row["mode"], case.mode.as_str());
        assert_eq!(row["state"], case.expected_state);
        assert_eq!(row["available"], case.expected_available);
        assert_eq!(row["source"], case.expected_source);
    }

    assert_eq!(
        matrix_rows,
        vec![
            "openai:api_key:ready:true",
            "anthropic:api_key:ready:true",
            "google:api_key:ready:true",
            "openai:oauth_token:ready:true",
            "openai:session_token:ready:true",
            "anthropic:oauth_token:ready:true",
            "google:session_token:unsupported_mode:false",
        ]
    );
}

#[test]
fn functional_execute_auth_command_matrix_reports_provider_mode_inventory() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-matrix.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = Some("shared-api-key".to_string());
    config.openai_api_key = None;
    config.anthropic_api_key = None;
    config.google_api_key = None;

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("oauth-access-secret".to_string()),
            refresh_token: Some("oauth-refresh-secret".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(600)),
            revoked: false,
        },
    );

    let output = execute_auth_command(&config, "matrix --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse matrix payload");
    assert_eq!(payload["command"], "auth.matrix");
    assert_eq!(payload["provider_filter"], "all");
    assert_eq!(payload["mode_filter"], "all");
    assert_eq!(payload["source_kind_filter"], "all");
    assert_eq!(payload["revoked_filter"], "all");
    assert_eq!(payload["providers"], 3);
    assert_eq!(payload["modes"], 4);
    assert_eq!(payload["rows_total"], 12);
    assert_eq!(payload["rows"], 12);
    assert_eq!(payload["mode_supported"], 9);
    assert_eq!(payload["mode_unsupported"], 3);
    assert_eq!(payload["mode_supported_total"], 9);
    assert_eq!(payload["mode_unsupported_total"], 3);
    assert_eq!(payload["mode_counts_total"]["api_key"], 3);
    assert_eq!(payload["mode_counts_total"]["oauth_token"], 3);
    assert_eq!(payload["mode_counts_total"]["adc"], 3);
    assert_eq!(payload["mode_counts_total"]["session_token"], 3);
    assert_eq!(payload["mode_counts"]["api_key"], 3);
    assert_eq!(payload["mode_counts"]["oauth_token"], 3);
    assert_eq!(payload["mode_counts"]["adc"], 3);
    assert_eq!(payload["mode_counts"]["session_token"], 3);
    assert_eq!(payload["provider_counts_total"]["openai"], 4);
    assert_eq!(payload["provider_counts_total"]["anthropic"], 4);
    assert_eq!(payload["provider_counts_total"]["google"], 4);
    assert_eq!(payload["provider_counts"]["openai"], 4);
    assert_eq!(payload["provider_counts"]["anthropic"], 4);
    assert_eq!(payload["provider_counts"]["google"], 4);
    assert_eq!(payload["available"], 8);
    assert_eq!(payload["unavailable"], 4);
    assert_eq!(payload["availability_counts_total"]["available"], 8);
    assert_eq!(payload["availability_counts_total"]["unavailable"], 4);
    assert_eq!(payload["availability_counts"]["available"], 8);
    assert_eq!(payload["availability_counts"]["unavailable"], 4);
    assert_eq!(payload["state_counts_total"]["ready"], 8);
    assert_eq!(payload["state_counts_total"]["mode_mismatch"], 1);
    assert_eq!(payload["state_counts_total"]["unsupported_mode"], 3);
    assert_eq!(payload["state_counts"]["ready"], 8);
    assert_eq!(payload["state_counts"]["mode_mismatch"], 1);
    assert_eq!(payload["state_counts"]["unsupported_mode"], 3);
    assert_eq!(payload["source_kind_counts_total"]["flag"], 3);
    assert_eq!(payload["source_kind_counts_total"]["credential_store"], 2);
    assert_eq!(payload["source_kind_counts_total"]["env"], 4);
    assert_eq!(payload["source_kind_counts_total"]["none"], 3);
    assert_eq!(payload["source_kind_counts"]["flag"], 3);
    assert_eq!(payload["source_kind_counts"]["credential_store"], 2);
    assert_eq!(payload["source_kind_counts"]["env"], 4);
    assert_eq!(payload["source_kind_counts"]["none"], 3);
    assert_eq!(payload["revoked_counts_total"]["not_revoked"], 12);
    assert_eq!(payload["revoked_counts"]["not_revoked"], 12);

    let entries = payload["entries"].as_array().expect("matrix entries");
    assert_eq!(entries.len(), 12);
    let row_for = |provider: &str, mode: &str| -> &serde_json::Value {
        entries
            .iter()
            .find(|row| row["provider"] == provider && row["mode"] == mode)
            .unwrap_or_else(|| panic!("missing matrix row provider={provider} mode={mode}"))
    };

    let openai_api = row_for("openai", "api_key");
    assert_eq!(openai_api["mode_supported"], true);
    assert_eq!(openai_api["available"], true);
    assert_eq!(openai_api["state"], "ready");

    let openai_oauth = row_for("openai", "oauth_token");
    assert_eq!(openai_oauth["mode_supported"], true);
    assert_eq!(openai_oauth["available"], true);
    assert_eq!(openai_oauth["state"], "ready");
    assert_eq!(openai_oauth["source"], "credential_store");

    let anthropic_oauth = row_for("anthropic", "oauth_token");
    assert_eq!(anthropic_oauth["mode_supported"], true);
    assert_eq!(anthropic_oauth["available"], true);
    assert_eq!(anthropic_oauth["state"], "ready");
    assert_eq!(anthropic_oauth["source"], "claude_cli");

    let google_oauth = row_for("google", "oauth_token");
    assert_eq!(google_oauth["mode_supported"], true);
    assert_eq!(google_oauth["available"], true);
    assert_eq!(google_oauth["state"], "ready");
    assert_eq!(google_oauth["source"], "gemini_cli");

    let text_output = execute_auth_command(&config, "matrix");
    assert!(text_output.contains("auth matrix: providers=3 modes=4 rows=12"));
    assert!(text_output.contains("mode_supported_total=9"));
    assert!(text_output.contains("mode_unsupported_total=3"));
    assert!(text_output.contains("mode_counts=adc:3,api_key:3,oauth_token:3,session_token:3"));
    assert!(text_output.contains("mode_counts_total=adc:3,api_key:3,oauth_token:3,session_token:3"));
    assert!(text_output.contains("provider_counts=anthropic:4,google:4,openai:4"));
    assert!(text_output.contains("provider_counts_total=anthropic:4,google:4,openai:4"));
    assert!(text_output.contains("availability_counts=available:8,unavailable:4"));
    assert!(text_output.contains("availability_counts_total=available:8,unavailable:4"));
    assert!(text_output.contains("provider_filter=all"));
    assert!(text_output.contains("mode_filter=all"));
    assert!(text_output.contains("source_kind_filter=all"));
    assert!(text_output.contains("revoked_filter=all"));
    assert!(text_output.contains("source_kind_counts=credential_store:2,env:4,flag:3,none:3"));
    assert!(text_output.contains("source_kind_counts_total=credential_store:2,env:4,flag:3,none:3"));
    assert!(text_output.contains("revoked_counts=not_revoked:12"));
    assert!(text_output.contains("revoked_counts_total=not_revoked:12"));
    assert!(text_output.contains("state_counts=mode_mismatch:1,ready:8,unsupported_mode:3"));
    assert!(text_output.contains("state_counts_total=mode_mismatch:1,ready:8,unsupported_mode:3"));
    assert!(text_output.contains("auth matrix row: provider=openai mode=oauth_token"));
    assert!(!text_output.contains("oauth-access-secret"));
}

#[test]
fn functional_execute_auth_command_matrix_supports_provider_and_mode_filters() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-matrix-filtered.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = Some("shared-api-key".to_string());

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("filtered-oauth-access".to_string()),
            refresh_token: Some("filtered-oauth-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(600)),
            revoked: false,
        },
    );

    let json_output = execute_auth_command(&config, "matrix openai --mode oauth-token --json");
    let payload: serde_json::Value =
        serde_json::from_str(&json_output).expect("parse filtered matrix payload");
    assert_eq!(payload["command"], "auth.matrix");
    assert_eq!(payload["provider_filter"], "openai");
    assert_eq!(payload["mode_filter"], "oauth_token");
    assert_eq!(payload["source_kind_filter"], "all");
    assert_eq!(payload["revoked_filter"], "all");
    assert_eq!(payload["providers"], 1);
    assert_eq!(payload["modes"], 1);
    assert_eq!(payload["rows_total"], 1);
    assert_eq!(payload["rows"], 1);
    assert_eq!(payload["mode_supported"], 1);
    assert_eq!(payload["mode_unsupported"], 0);
    assert_eq!(payload["mode_supported_total"], 1);
    assert_eq!(payload["mode_unsupported_total"], 0);
    assert_eq!(payload["mode_counts_total"]["oauth_token"], 1);
    assert_eq!(payload["mode_counts"]["oauth_token"], 1);
    assert_eq!(payload["provider_counts_total"]["openai"], 1);
    assert_eq!(payload["provider_counts"]["openai"], 1);
    assert_eq!(payload["available"], 1);
    assert_eq!(payload["source_kind_counts_total"]["credential_store"], 1);
    assert_eq!(payload["source_kind_counts"]["credential_store"], 1);
    let entries = payload["entries"].as_array().expect("entries");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["provider"], "openai");
    assert_eq!(entries[0]["mode"], "oauth_token");
    assert_eq!(entries[0]["state"], "ready");
    assert_eq!(entries[0]["available"], true);

    let text_output = execute_auth_command(&config, "matrix openai --mode oauth-token");
    assert!(text_output.contains("auth matrix: providers=1 modes=1 rows=1"));
    assert!(text_output.contains("provider_filter=openai"));
    assert!(text_output.contains("mode_filter=oauth_token"));
    assert!(text_output.contains("source_kind_filter=all"));
    assert!(text_output.contains("revoked_filter=all"));
    assert!(text_output.contains("provider_counts=openai:1"));
    assert!(text_output.contains("provider_counts_total=openai:1"));
    assert!(text_output.contains(
        "auth matrix row: provider=openai mode=oauth_token mode_supported=true available=true state=ready"
    ));
}

#[test]
fn functional_execute_auth_command_matrix_supports_availability_filter() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-matrix-availability.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = Some("shared-api-key".to_string());

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("availability-access".to_string()),
            refresh_token: Some("availability-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(600)),
            revoked: false,
        },
    );

    let available_output = execute_auth_command(&config, "matrix --availability available --json");
    let available_payload: serde_json::Value =
        serde_json::from_str(&available_output).expect("parse available matrix payload");
    assert_eq!(available_payload["provider_filter"], "all");
    assert_eq!(available_payload["mode_filter"], "all");
    assert_eq!(available_payload["source_kind_filter"], "all");
    assert_eq!(available_payload["revoked_filter"], "all");
    assert_eq!(available_payload["availability_filter"], "available");
    assert_eq!(available_payload["rows_total"], 12);
    assert_eq!(available_payload["rows"], 8);
    assert_eq!(available_payload["mode_supported_total"], 9);
    assert_eq!(available_payload["mode_unsupported_total"], 3);
    assert_eq!(available_payload["mode_counts_total"]["api_key"], 3);
    assert_eq!(available_payload["mode_counts_total"]["oauth_token"], 3);
    assert_eq!(available_payload["mode_counts_total"]["adc"], 3);
    assert_eq!(available_payload["mode_counts_total"]["session_token"], 3);
    assert_eq!(available_payload["mode_counts"]["api_key"], 3);
    assert_eq!(available_payload["mode_counts"]["oauth_token"], 3);
    assert_eq!(available_payload["mode_counts"]["adc"], 1);
    assert_eq!(available_payload["mode_counts"]["session_token"], 1);
    assert_eq!(available_payload["provider_counts_total"]["openai"], 4);
    assert_eq!(available_payload["provider_counts_total"]["anthropic"], 4);
    assert_eq!(available_payload["provider_counts_total"]["google"], 4);
    assert_eq!(available_payload["provider_counts"]["openai"], 2);
    assert_eq!(available_payload["provider_counts"]["anthropic"], 3);
    assert_eq!(available_payload["provider_counts"]["google"], 3);
    assert_eq!(
        available_payload["availability_counts_total"]["available"],
        8
    );
    assert_eq!(
        available_payload["availability_counts_total"]["unavailable"],
        4
    );
    assert_eq!(available_payload["availability_counts"]["available"], 8);
    assert_eq!(available_payload["source_kind_counts_total"]["flag"], 3);
    assert_eq!(
        available_payload["source_kind_counts_total"]["credential_store"],
        2
    );
    assert_eq!(available_payload["source_kind_counts_total"]["env"], 4);
    assert_eq!(available_payload["source_kind_counts_total"]["none"], 3);
    assert_eq!(available_payload["source_kind_counts"]["flag"], 3);
    assert_eq!(
        available_payload["source_kind_counts"]["credential_store"],
        1
    );
    assert_eq!(available_payload["source_kind_counts"]["env"], 4);
    assert_eq!(available_payload["revoked_counts_total"]["not_revoked"], 12);
    assert_eq!(available_payload["revoked_counts"]["not_revoked"], 8);
    assert_eq!(available_payload["available"], 8);
    assert_eq!(available_payload["unavailable"], 0);
    assert_eq!(available_payload["state_counts_total"]["ready"], 8);
    assert_eq!(available_payload["state_counts_total"]["mode_mismatch"], 1);
    assert_eq!(
        available_payload["state_counts_total"]["unsupported_mode"],
        3
    );
    assert_eq!(available_payload["state_counts"]["ready"], 8);
    let available_entries = available_payload["entries"]
        .as_array()
        .expect("available entries");
    assert_eq!(available_entries.len(), 8);
    assert!(available_entries
        .iter()
        .all(|entry| entry["available"].as_bool() == Some(true)));

    let unavailable_output =
        execute_auth_command(&config, "matrix --availability unavailable --json");
    let unavailable_payload: serde_json::Value =
        serde_json::from_str(&unavailable_output).expect("parse unavailable matrix payload");
    assert_eq!(unavailable_payload["provider_filter"], "all");
    assert_eq!(unavailable_payload["mode_filter"], "all");
    assert_eq!(unavailable_payload["source_kind_filter"], "all");
    assert_eq!(unavailable_payload["revoked_filter"], "all");
    assert_eq!(unavailable_payload["availability_filter"], "unavailable");
    assert_eq!(unavailable_payload["rows_total"], 12);
    assert_eq!(unavailable_payload["rows"], 4);
    assert_eq!(unavailable_payload["mode_supported_total"], 9);
    assert_eq!(unavailable_payload["mode_unsupported_total"], 3);
    assert_eq!(unavailable_payload["mode_counts_total"]["api_key"], 3);
    assert_eq!(unavailable_payload["mode_counts_total"]["oauth_token"], 3);
    assert_eq!(unavailable_payload["mode_counts_total"]["adc"], 3);
    assert_eq!(unavailable_payload["mode_counts_total"]["session_token"], 3);
    assert!(unavailable_payload["mode_counts"]["oauth_token"].is_null());
    assert_eq!(unavailable_payload["mode_counts"]["adc"], 2);
    assert_eq!(unavailable_payload["mode_counts"]["session_token"], 2);
    assert_eq!(unavailable_payload["provider_counts_total"]["openai"], 4);
    assert_eq!(unavailable_payload["provider_counts_total"]["anthropic"], 4);
    assert_eq!(unavailable_payload["provider_counts_total"]["google"], 4);
    assert_eq!(unavailable_payload["provider_counts"]["openai"], 2);
    assert_eq!(unavailable_payload["provider_counts"]["anthropic"], 1);
    assert_eq!(unavailable_payload["provider_counts"]["google"], 1);
    assert_eq!(
        unavailable_payload["availability_counts_total"]["available"],
        8
    );
    assert_eq!(
        unavailable_payload["availability_counts_total"]["unavailable"],
        4
    );
    assert_eq!(unavailable_payload["availability_counts"]["unavailable"], 4);
    assert_eq!(unavailable_payload["source_kind_counts_total"]["flag"], 3);
    assert_eq!(
        unavailable_payload["source_kind_counts_total"]["credential_store"],
        2
    );
    assert_eq!(unavailable_payload["source_kind_counts_total"]["env"], 4);
    assert_eq!(unavailable_payload["source_kind_counts_total"]["none"], 3);
    assert_eq!(
        unavailable_payload["source_kind_counts"]["credential_store"],
        1
    );
    assert_eq!(unavailable_payload["source_kind_counts"]["none"], 3);
    assert_eq!(
        unavailable_payload["revoked_counts_total"]["not_revoked"],
        12
    );
    assert_eq!(unavailable_payload["revoked_counts"]["not_revoked"], 4);
    assert_eq!(unavailable_payload["available"], 0);
    assert_eq!(unavailable_payload["unavailable"], 4);
    assert_eq!(unavailable_payload["state_counts_total"]["ready"], 8);
    assert_eq!(
        unavailable_payload["state_counts_total"]["mode_mismatch"],
        1
    );
    assert_eq!(
        unavailable_payload["state_counts_total"]["unsupported_mode"],
        3
    );
    assert_eq!(unavailable_payload["state_counts"]["mode_mismatch"], 1);
    assert_eq!(unavailable_payload["state_counts"]["unsupported_mode"], 3);
    let unavailable_entries = unavailable_payload["entries"]
        .as_array()
        .expect("unavailable entries");
    assert_eq!(unavailable_entries.len(), 4);
    assert!(unavailable_entries
        .iter()
        .all(|entry| entry["available"].as_bool() == Some(false)));
}

#[test]
fn functional_execute_auth_command_matrix_supports_state_filter() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-matrix-state-filter.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = Some("shared-api-key".to_string());

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("state-filter-access".to_string()),
            refresh_token: Some("state-filter-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(600)),
            revoked: false,
        },
    );

    let ready_output = execute_auth_command(&config, "matrix --state ready --json");
    let ready_payload: serde_json::Value =
        serde_json::from_str(&ready_output).expect("parse state-filtered payload");
    assert_eq!(ready_payload["command"], "auth.matrix");
    assert_eq!(ready_payload["provider_filter"], "all");
    assert_eq!(ready_payload["mode_filter"], "all");
    assert_eq!(ready_payload["state_filter"], "ready");
    assert_eq!(ready_payload["source_kind_filter"], "all");
    assert_eq!(ready_payload["revoked_filter"], "all");
    assert_eq!(ready_payload["rows_total"], 12);
    assert_eq!(ready_payload["rows"], 8);
    assert_eq!(ready_payload["mode_supported_total"], 9);
    assert_eq!(ready_payload["mode_unsupported_total"], 3);
    assert_eq!(ready_payload["provider_counts_total"]["openai"], 4);
    assert_eq!(ready_payload["provider_counts_total"]["anthropic"], 4);
    assert_eq!(ready_payload["provider_counts_total"]["google"], 4);
    assert_eq!(ready_payload["provider_counts"]["openai"], 2);
    assert_eq!(ready_payload["provider_counts"]["anthropic"], 3);
    assert_eq!(ready_payload["provider_counts"]["google"], 3);
    assert_eq!(ready_payload["source_kind_counts_total"]["flag"], 3);
    assert_eq!(
        ready_payload["source_kind_counts_total"]["credential_store"],
        2
    );
    assert_eq!(ready_payload["source_kind_counts_total"]["env"], 4);
    assert_eq!(ready_payload["source_kind_counts_total"]["none"], 3);
    assert_eq!(ready_payload["source_kind_counts"]["flag"], 3);
    assert_eq!(ready_payload["source_kind_counts"]["credential_store"], 1);
    assert_eq!(ready_payload["source_kind_counts"]["env"], 4);
    assert_eq!(ready_payload["state_counts_total"]["ready"], 8);
    assert_eq!(ready_payload["state_counts_total"]["mode_mismatch"], 1);
    assert_eq!(ready_payload["state_counts_total"]["unsupported_mode"], 3);
    assert_eq!(ready_payload["state_counts"]["ready"], 8);
    let ready_entries = ready_payload["entries"].as_array().expect("ready entries");
    assert_eq!(ready_entries.len(), 8);
    assert!(ready_entries.iter().all(|entry| entry["state"] == "ready"));

    let text_output = execute_auth_command(&config, "matrix --state ready");
    assert!(text_output.contains("provider_filter=all"));
    assert!(text_output.contains("mode_filter=all"));
    assert!(text_output.contains("state_filter=ready"));
    assert!(text_output.contains("source_kind_filter=all"));
    assert!(text_output.contains("revoked_filter=all"));
    assert!(text_output.contains("provider_counts=anthropic:3,google:3,openai:2"));
    assert!(text_output.contains("provider_counts_total=anthropic:4,google:4,openai:4"));
    assert!(text_output.contains("source_kind_counts=credential_store:1,env:4,flag:3"));
    assert!(text_output.contains("source_kind_counts_total=credential_store:2,env:4,flag:3,none:3"));
    assert!(text_output.contains("state_counts=ready:8"));
    assert!(text_output.contains("state_counts_total=mode_mismatch:1,ready:8,unsupported_mode:3"));
    assert!(!text_output.contains("state=unsupported_mode"));
}

#[test]
fn functional_execute_auth_command_matrix_supports_mode_support_filter() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-matrix-mode-support-filter.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = Some("shared-api-key".to_string());

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("mode-support-access".to_string()),
            refresh_token: Some("mode-support-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(600)),
            revoked: false,
        },
    );

    let supported_output = execute_auth_command(&config, "matrix --mode-support supported --json");
    let supported_payload: serde_json::Value =
        serde_json::from_str(&supported_output).expect("parse supported-only matrix payload");
    assert_eq!(supported_payload["provider_filter"], "all");
    assert_eq!(supported_payload["mode_filter"], "all");
    assert_eq!(supported_payload["mode_support_filter"], "supported");
    assert_eq!(supported_payload["source_kind_filter"], "all");
    assert_eq!(supported_payload["revoked_filter"], "all");
    assert_eq!(supported_payload["rows_total"], 12);
    assert_eq!(supported_payload["rows"], 9);
    assert_eq!(supported_payload["mode_supported"], 9);
    assert_eq!(supported_payload["mode_unsupported"], 0);
    assert_eq!(supported_payload["mode_supported_total"], 9);
    assert_eq!(supported_payload["mode_unsupported_total"], 3);
    assert_eq!(supported_payload["provider_counts_total"]["openai"], 4);
    assert_eq!(supported_payload["provider_counts_total"]["anthropic"], 4);
    assert_eq!(supported_payload["provider_counts_total"]["google"], 4);
    assert_eq!(supported_payload["provider_counts"]["openai"], 3);
    assert_eq!(supported_payload["provider_counts"]["anthropic"], 3);
    assert_eq!(supported_payload["provider_counts"]["google"], 3);
    assert_eq!(supported_payload["source_kind_counts_total"]["flag"], 3);
    assert_eq!(
        supported_payload["source_kind_counts_total"]["credential_store"],
        2
    );
    assert_eq!(supported_payload["source_kind_counts_total"]["env"], 4);
    assert_eq!(supported_payload["source_kind_counts_total"]["none"], 3);
    assert_eq!(supported_payload["source_kind_counts"]["flag"], 3);
    assert_eq!(
        supported_payload["source_kind_counts"]["credential_store"],
        2
    );
    assert_eq!(supported_payload["source_kind_counts"]["env"], 4);
    assert_eq!(supported_payload["state_counts_total"]["ready"], 8);
    assert_eq!(supported_payload["state_counts_total"]["mode_mismatch"], 1);
    assert_eq!(
        supported_payload["state_counts_total"]["unsupported_mode"],
        3
    );
    assert_eq!(supported_payload["state_counts"]["ready"], 8);
    assert_eq!(supported_payload["state_counts"]["mode_mismatch"], 1);
    let supported_entries = supported_payload["entries"]
        .as_array()
        .expect("supported entries");
    assert_eq!(supported_entries.len(), 9);
    assert!(supported_entries
        .iter()
        .all(|entry| entry["mode_supported"].as_bool() == Some(true)));

    let unsupported_output =
        execute_auth_command(&config, "matrix --mode-support unsupported --json");
    let unsupported_payload: serde_json::Value =
        serde_json::from_str(&unsupported_output).expect("parse unsupported-only matrix payload");
    assert_eq!(unsupported_payload["provider_filter"], "all");
    assert_eq!(unsupported_payload["mode_filter"], "all");
    assert_eq!(unsupported_payload["mode_support_filter"], "unsupported");
    assert_eq!(unsupported_payload["source_kind_filter"], "all");
    assert_eq!(unsupported_payload["revoked_filter"], "all");
    assert_eq!(unsupported_payload["rows_total"], 12);
    assert_eq!(unsupported_payload["rows"], 3);
    assert_eq!(unsupported_payload["mode_supported"], 0);
    assert_eq!(unsupported_payload["mode_unsupported"], 3);
    assert_eq!(unsupported_payload["mode_supported_total"], 9);
    assert_eq!(unsupported_payload["mode_unsupported_total"], 3);
    assert_eq!(unsupported_payload["provider_counts_total"]["openai"], 4);
    assert_eq!(unsupported_payload["provider_counts_total"]["anthropic"], 4);
    assert_eq!(unsupported_payload["provider_counts_total"]["google"], 4);
    assert_eq!(unsupported_payload["provider_counts"]["openai"], 1);
    assert_eq!(unsupported_payload["provider_counts"]["anthropic"], 1);
    assert_eq!(unsupported_payload["provider_counts"]["google"], 1);
    assert_eq!(unsupported_payload["source_kind_counts_total"]["flag"], 3);
    assert_eq!(
        unsupported_payload["source_kind_counts_total"]["credential_store"],
        2
    );
    assert_eq!(unsupported_payload["source_kind_counts_total"]["env"], 4);
    assert_eq!(unsupported_payload["source_kind_counts_total"]["none"], 3);
    assert_eq!(unsupported_payload["source_kind_counts"]["none"], 3);
    assert_eq!(unsupported_payload["state_counts_total"]["ready"], 8);
    assert_eq!(
        unsupported_payload["state_counts_total"]["mode_mismatch"],
        1
    );
    assert_eq!(
        unsupported_payload["state_counts_total"]["unsupported_mode"],
        3
    );
    assert_eq!(unsupported_payload["state_counts"]["unsupported_mode"], 3);
    let unsupported_entries = unsupported_payload["entries"]
        .as_array()
        .expect("unsupported entries");
    assert_eq!(unsupported_entries.len(), 3);
    assert!(unsupported_entries
        .iter()
        .all(|entry| entry["mode_supported"].as_bool() == Some(false)));

    let text_output = execute_auth_command(&config, "matrix --mode-support supported");
    assert!(text_output.contains("provider_filter=all"));
    assert!(text_output.contains("mode_filter=all"));
    assert!(text_output.contains("mode_support_filter=supported"));
    assert!(text_output.contains("source_kind_filter=all"));
    assert!(text_output.contains("revoked_filter=all"));
    assert!(text_output.contains("mode_supported_total=9"));
    assert!(text_output.contains("mode_unsupported_total=3"));
    assert!(text_output.contains("source_kind_counts=credential_store:2,env:4,flag:3"));
    assert!(text_output.contains("source_kind_counts_total=credential_store:2,env:4,flag:3,none:3"));
    assert!(text_output.contains("state_counts=mode_mismatch:1,ready:8"));
    assert!(text_output.contains("state_counts_total=mode_mismatch:1,ready:8,unsupported_mode:3"));
    assert!(!text_output.contains("mode_supported=false"));
}

#[test]
fn functional_execute_auth_command_matrix_supports_source_kind_filter() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-matrix-source-kind-filter.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = Some("shared-api-key".to_string());

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("source-kind-access".to_string()),
            refresh_token: Some("source-kind-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(600)),
            revoked: false,
        },
    );

    let filtered_output =
        execute_auth_command(&config, "matrix --source-kind credential-store --json");
    let filtered_payload: serde_json::Value =
        serde_json::from_str(&filtered_output).expect("parse source-kind filtered matrix payload");
    assert_eq!(filtered_payload["provider_filter"], "all");
    assert_eq!(filtered_payload["mode_filter"], "all");
    assert_eq!(filtered_payload["source_kind_filter"], "credential_store");
    assert_eq!(filtered_payload["revoked_filter"], "all");
    assert_eq!(filtered_payload["rows_total"], 12);
    assert_eq!(filtered_payload["rows"], 2);
    assert_eq!(filtered_payload["mode_supported"], 2);
    assert_eq!(filtered_payload["mode_unsupported"], 0);
    assert_eq!(filtered_payload["available"], 1);
    assert_eq!(filtered_payload["unavailable"], 1);
    assert_eq!(filtered_payload["provider_counts_total"]["openai"], 4);
    assert_eq!(filtered_payload["provider_counts_total"]["anthropic"], 4);
    assert_eq!(filtered_payload["provider_counts_total"]["google"], 4);
    assert_eq!(filtered_payload["provider_counts"]["openai"], 2);
    assert_eq!(
        filtered_payload["source_kind_counts_total"]["credential_store"],
        2
    );
    assert_eq!(filtered_payload["source_kind_counts_total"]["flag"], 3);
    assert_eq!(filtered_payload["source_kind_counts_total"]["env"], 4);
    assert_eq!(filtered_payload["source_kind_counts_total"]["none"], 3);
    assert_eq!(
        filtered_payload["source_kind_counts"]["credential_store"],
        2
    );
    assert_eq!(filtered_payload["state_counts"]["ready"], 1);
    assert_eq!(filtered_payload["state_counts"]["mode_mismatch"], 1);
    let filtered_entries = filtered_payload["entries"]
        .as_array()
        .expect("source-kind filtered entries");
    assert_eq!(filtered_entries.len(), 2);
    assert!(filtered_entries
        .iter()
        .all(|entry| entry["source"] == "credential_store"));

    let text_output = execute_auth_command(&config, "matrix --source-kind credential-store");
    assert!(text_output.contains("source_kind_filter=credential_store"));
    assert!(text_output.contains("revoked_filter=all"));
    assert!(text_output.contains("rows=2"));
    assert!(text_output.contains("provider_counts=openai:2"));
    assert!(text_output.contains("provider_counts_total=anthropic:4,google:4,openai:4"));
    assert!(text_output.contains("source_kind_counts=credential_store:2"));
}

#[test]
fn functional_execute_auth_command_matrix_supports_revoked_filter() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-matrix-revoked-filter.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = Some("shared-api-key".to_string());

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::SessionToken,
            access_token: Some("matrix-revoked-access".to_string()),
            refresh_token: Some("matrix-revoked-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(600)),
            revoked: true,
        },
    );

    let revoked_output = execute_auth_command(&config, "matrix openai --revoked revoked --json");
    let revoked_payload: serde_json::Value =
        serde_json::from_str(&revoked_output).expect("parse revoked matrix payload");
    assert_eq!(revoked_payload["provider_filter"], "openai");
    assert_eq!(revoked_payload["mode_filter"], "all");
    assert_eq!(revoked_payload["revoked_filter"], "revoked");
    assert_eq!(revoked_payload["rows_total"], 4);
    assert_eq!(revoked_payload["rows"], 2);
    assert_eq!(revoked_payload["mode_supported"], 2);
    assert_eq!(revoked_payload["mode_unsupported"], 0);
    assert_eq!(revoked_payload["mode_counts_total"]["api_key"], 1);
    assert_eq!(revoked_payload["mode_counts_total"]["oauth_token"], 1);
    assert_eq!(revoked_payload["mode_counts_total"]["adc"], 1);
    assert_eq!(revoked_payload["mode_counts_total"]["session_token"], 1);
    assert_eq!(revoked_payload["mode_counts"]["oauth_token"], 1);
    assert_eq!(revoked_payload["mode_counts"]["session_token"], 1);
    assert_eq!(revoked_payload["provider_counts_total"]["openai"], 4);
    assert_eq!(revoked_payload["provider_counts"]["openai"], 2);
    assert_eq!(revoked_payload["available"], 0);
    assert_eq!(revoked_payload["unavailable"], 2);
    assert_eq!(revoked_payload["state_counts"]["mode_mismatch"], 1);
    assert_eq!(revoked_payload["state_counts"]["revoked"], 1);
    assert_eq!(revoked_payload["source_kind_counts"]["credential_store"], 2);
    assert_eq!(revoked_payload["revoked_counts_total"]["not_revoked"], 2);
    assert_eq!(revoked_payload["revoked_counts_total"]["revoked"], 2);
    assert_eq!(revoked_payload["revoked_counts"]["revoked"], 2);
    let revoked_entries = revoked_payload["entries"]
        .as_array()
        .expect("revoked matrix entries");
    assert_eq!(revoked_entries.len(), 2);
    assert!(revoked_entries.iter().all(|entry| entry["revoked"] == true));

    let not_revoked_output =
        execute_auth_command(&config, "matrix openai --revoked not-revoked --json");
    let not_revoked_payload: serde_json::Value =
        serde_json::from_str(&not_revoked_output).expect("parse non-revoked matrix payload");
    assert_eq!(not_revoked_payload["revoked_filter"], "not_revoked");
    assert_eq!(not_revoked_payload["rows_total"], 4);
    assert_eq!(not_revoked_payload["rows"], 2);
    assert_eq!(not_revoked_payload["mode_counts_total"]["api_key"], 1);
    assert_eq!(not_revoked_payload["mode_counts_total"]["oauth_token"], 1);
    assert_eq!(not_revoked_payload["mode_counts_total"]["adc"], 1);
    assert_eq!(not_revoked_payload["mode_counts_total"]["session_token"], 1);
    assert_eq!(not_revoked_payload["mode_counts"]["api_key"], 1);
    assert_eq!(not_revoked_payload["mode_counts"]["adc"], 1);
    assert_eq!(not_revoked_payload["provider_counts_total"]["openai"], 4);
    assert_eq!(not_revoked_payload["provider_counts"]["openai"], 2);
    assert_eq!(
        not_revoked_payload["revoked_counts_total"]["not_revoked"],
        2
    );
    assert_eq!(not_revoked_payload["revoked_counts_total"]["revoked"], 2);
    assert_eq!(not_revoked_payload["revoked_counts"]["not_revoked"], 2);
    let not_revoked_entries = not_revoked_payload["entries"]
        .as_array()
        .expect("non-revoked matrix entries");
    assert_eq!(not_revoked_entries.len(), 2);
    assert!(not_revoked_entries
        .iter()
        .all(|entry| entry["revoked"] == false));

    let text_output = execute_auth_command(&config, "matrix openai --revoked revoked");
    assert!(text_output.contains("revoked_filter=revoked"));
    assert!(text_output.contains("rows=2"));
    assert!(text_output.contains("mode_counts=oauth_token:1,session_token:1"));
    assert!(text_output.contains("mode_counts_total=adc:1,api_key:1,oauth_token:1,session_token:1"));
    assert!(text_output.contains("provider_counts=openai:2"));
    assert!(text_output.contains("provider_counts_total=openai:4"));
    assert!(text_output.contains("revoked_counts=revoked:2"));
    assert!(text_output.contains("revoked_counts_total=not_revoked:2,revoked:2"));
}

#[test]
fn integration_execute_auth_command_matrix_state_filter_composes_with_other_filters() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-matrix-state-composition.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = Some("shared-api-key".to_string());

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("state-composition-access".to_string()),
            refresh_token: Some("state-composition-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(600)),
            revoked: false,
        },
    );

    let filtered_output = execute_auth_command(
        &config,
        "matrix openai --mode oauth-token --availability available --state ready --source-kind credential-store --json",
    );
    let filtered_payload: serde_json::Value =
        serde_json::from_str(&filtered_output).expect("parse composed filter payload");
    assert_eq!(filtered_payload["provider_filter"], "openai");
    assert_eq!(filtered_payload["mode_filter"], "oauth_token");
    assert_eq!(filtered_payload["availability_filter"], "available");
    assert_eq!(filtered_payload["state_filter"], "ready");
    assert_eq!(filtered_payload["source_kind_filter"], "credential_store");
    assert_eq!(filtered_payload["revoked_filter"], "all");
    assert_eq!(filtered_payload["providers"], 1);
    assert_eq!(filtered_payload["modes"], 1);
    assert_eq!(filtered_payload["rows_total"], 1);
    assert_eq!(filtered_payload["rows"], 1);
    assert_eq!(filtered_payload["mode_supported_total"], 1);
    assert_eq!(filtered_payload["mode_unsupported_total"], 0);
    assert_eq!(filtered_payload["mode_counts_total"]["oauth_token"], 1);
    assert_eq!(filtered_payload["mode_counts"]["oauth_token"], 1);
    assert_eq!(filtered_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(filtered_payload["provider_counts"]["openai"], 1);
    assert_eq!(
        filtered_payload["source_kind_counts_total"]["credential_store"],
        1
    );
    assert_eq!(
        filtered_payload["source_kind_counts"]["credential_store"],
        1
    );
    assert_eq!(filtered_payload["state_counts_total"]["ready"], 1);
    assert_eq!(filtered_payload["state_counts"]["ready"], 1);
    assert_eq!(filtered_payload["revoked_counts_total"]["not_revoked"], 1);
    assert_eq!(filtered_payload["revoked_counts"]["not_revoked"], 1);
    assert_eq!(filtered_payload["entries"][0]["provider"], "openai");
    assert_eq!(filtered_payload["entries"][0]["mode"], "oauth_token");
    assert_eq!(filtered_payload["entries"][0]["state"], "ready");
    assert_eq!(filtered_payload["entries"][0]["available"], true);

    let mismatch_output = execute_auth_command(
        &config,
        "matrix openai --mode session-token --state mode_mismatch --source-kind credential-store --json",
    );
    let mismatch_payload: serde_json::Value =
        serde_json::from_str(&mismatch_output).expect("parse mismatch filter payload");
    assert_eq!(mismatch_payload["provider_filter"], "openai");
    assert_eq!(mismatch_payload["mode_filter"], "session_token");
    assert_eq!(mismatch_payload["state_filter"], "mode_mismatch");
    assert_eq!(mismatch_payload["source_kind_filter"], "credential_store");
    assert_eq!(mismatch_payload["revoked_filter"], "all");
    assert_eq!(mismatch_payload["rows_total"], 1);
    assert_eq!(mismatch_payload["rows"], 1);
    assert_eq!(mismatch_payload["mode_supported_total"], 1);
    assert_eq!(mismatch_payload["mode_unsupported_total"], 0);
    assert_eq!(mismatch_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(mismatch_payload["provider_counts"]["openai"], 1);
    assert_eq!(
        mismatch_payload["source_kind_counts_total"]["credential_store"],
        1
    );
    assert_eq!(
        mismatch_payload["source_kind_counts"]["credential_store"],
        1
    );
    assert_eq!(mismatch_payload["state_counts_total"]["mode_mismatch"], 1);
    assert_eq!(mismatch_payload["state_counts"]["mode_mismatch"], 1);
    assert_eq!(mismatch_payload["entries"][0]["state"], "mode_mismatch");
}

#[test]
fn integration_execute_auth_command_matrix_mode_support_filter_composes_with_other_filters() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp
        .path()
        .join("auth-matrix-mode-support-composition.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = Some("shared-api-key".to_string());

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("mode-support-composition-access".to_string()),
            refresh_token: Some("mode-support-composition-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(600)),
            revoked: false,
        },
    );

    let filtered_output = execute_auth_command(
        &config,
        "matrix openai --mode oauth-token --mode-support supported --availability available --state ready --source-kind credential-store --json",
    );
    let filtered_payload: serde_json::Value =
        serde_json::from_str(&filtered_output).expect("parse mode-support composed filter payload");
    assert_eq!(filtered_payload["provider_filter"], "openai");
    assert_eq!(filtered_payload["mode_filter"], "oauth_token");
    assert_eq!(filtered_payload["mode_support_filter"], "supported");
    assert_eq!(filtered_payload["availability_filter"], "available");
    assert_eq!(filtered_payload["state_filter"], "ready");
    assert_eq!(filtered_payload["source_kind_filter"], "credential_store");
    assert_eq!(filtered_payload["revoked_filter"], "all");
    assert_eq!(filtered_payload["providers"], 1);
    assert_eq!(filtered_payload["modes"], 1);
    assert_eq!(filtered_payload["rows_total"], 1);
    assert_eq!(filtered_payload["rows"], 1);
    assert_eq!(filtered_payload["mode_supported_total"], 1);
    assert_eq!(filtered_payload["mode_unsupported_total"], 0);
    assert_eq!(filtered_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(filtered_payload["provider_counts"]["openai"], 1);
    assert_eq!(
        filtered_payload["source_kind_counts_total"]["credential_store"],
        1
    );
    assert_eq!(
        filtered_payload["source_kind_counts"]["credential_store"],
        1
    );
    assert_eq!(filtered_payload["state_counts_total"]["ready"], 1);
    assert_eq!(filtered_payload["state_counts"]["ready"], 1);
    assert_eq!(filtered_payload["entries"][0]["provider"], "openai");
    assert_eq!(filtered_payload["entries"][0]["mode"], "oauth_token");
    assert_eq!(filtered_payload["entries"][0]["mode_supported"], true);
    assert_eq!(filtered_payload["entries"][0]["state"], "ready");

    let zero_row_output = execute_auth_command(
        &config,
        "matrix openai --mode oauth-token --mode-support unsupported --source-kind credential-store --json",
    );
    let zero_row_payload: serde_json::Value =
        serde_json::from_str(&zero_row_output).expect("parse zero-row mode-support payload");
    assert_eq!(zero_row_payload["provider_filter"], "openai");
    assert_eq!(zero_row_payload["mode_filter"], "oauth_token");
    assert_eq!(zero_row_payload["mode_support_filter"], "unsupported");
    assert_eq!(zero_row_payload["source_kind_filter"], "credential_store");
    assert_eq!(zero_row_payload["revoked_filter"], "all");
    assert_eq!(zero_row_payload["rows_total"], 1);
    assert_eq!(zero_row_payload["rows"], 0);
    assert_eq!(zero_row_payload["mode_supported"], 0);
    assert_eq!(zero_row_payload["mode_unsupported"], 0);
    assert_eq!(zero_row_payload["mode_supported_total"], 1);
    assert_eq!(zero_row_payload["mode_unsupported_total"], 0);
    assert_eq!(zero_row_payload["mode_counts_total"]["oauth_token"], 1);
    assert_eq!(
        zero_row_payload["mode_counts"]
            .as_object()
            .expect("zero-row mode counts")
            .len(),
        0
    );
    assert_eq!(zero_row_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(
        zero_row_payload["provider_counts"]
            .as_object()
            .expect("zero-row provider counts")
            .len(),
        0
    );
    assert_eq!(
        zero_row_payload["source_kind_counts_total"]["credential_store"],
        1
    );
    assert_eq!(
        zero_row_payload["source_kind_counts"]
            .as_object()
            .expect("zero-row source-kind counts")
            .len(),
        0
    );
    assert_eq!(zero_row_payload["state_counts_total"]["ready"], 1);
    assert_eq!(zero_row_payload["revoked_counts_total"]["not_revoked"], 1);
    assert_eq!(
        zero_row_payload["revoked_counts"]
            .as_object()
            .expect("zero-row revoked counts")
            .len(),
        0
    );
    assert_eq!(
        zero_row_payload["state_counts"]
            .as_object()
            .expect("zero-row state counts")
            .len(),
        0
    );
    assert_eq!(
        zero_row_payload["entries"]
            .as_array()
            .expect("zero-row entries")
            .len(),
        0
    );

    let zero_row_text = execute_auth_command(
        &config,
        "matrix openai --mode oauth-token --mode-support unsupported --source-kind credential-store",
    );
    assert!(zero_row_text.contains("rows=0"));
    assert!(zero_row_text.contains("mode_supported_total=1"));
    assert!(zero_row_text.contains("mode_unsupported_total=0"));
    assert!(zero_row_text.contains("mode_counts=none"));
    assert!(zero_row_text.contains("mode_counts_total=oauth_token:1"));
    assert!(zero_row_text.contains("provider_counts=none"));
    assert!(zero_row_text.contains("provider_counts_total=openai:1"));
    assert!(zero_row_text.contains("provider_filter=openai"));
    assert!(zero_row_text.contains("mode_filter=oauth_token"));
    assert!(zero_row_text.contains("source_kind_filter=credential_store"));
    assert!(zero_row_text.contains("revoked_filter=all"));
    assert!(zero_row_text.contains("source_kind_counts=none"));
    assert!(zero_row_text.contains("source_kind_counts_total=credential_store:1"));
    assert!(zero_row_text.contains("revoked_counts=none"));
    assert!(zero_row_text.contains("revoked_counts_total=not_revoked:1"));
    assert!(zero_row_text.contains("state_counts=none"));
    assert!(zero_row_text.contains("state_counts_total=ready:1"));
}

#[test]
fn integration_execute_auth_command_matrix_revoked_filter_composes_with_other_filters() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-matrix-revoked-composition.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = Some("shared-api-key".to_string());

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::SessionToken,
            access_token: Some("matrix-revoked-composition-access".to_string()),
            refresh_token: Some("matrix-revoked-composition-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(600)),
            revoked: true,
        },
    );

    let revoked_output = execute_auth_command(
        &config,
        "matrix openai --mode session-token --mode-support supported --availability unavailable --state revoked --source-kind credential-store --revoked revoked --json",
    );
    let revoked_payload: serde_json::Value =
        serde_json::from_str(&revoked_output).expect("parse revoked composed matrix payload");
    assert_eq!(revoked_payload["provider_filter"], "openai");
    assert_eq!(revoked_payload["mode_filter"], "session_token");
    assert_eq!(revoked_payload["mode_support_filter"], "supported");
    assert_eq!(revoked_payload["availability_filter"], "unavailable");
    assert_eq!(revoked_payload["state_filter"], "revoked");
    assert_eq!(revoked_payload["source_kind_filter"], "credential_store");
    assert_eq!(revoked_payload["revoked_filter"], "revoked");
    assert_eq!(revoked_payload["rows_total"], 1);
    assert_eq!(revoked_payload["rows"], 1);
    assert_eq!(revoked_payload["mode_counts_total"]["session_token"], 1);
    assert_eq!(revoked_payload["mode_counts"]["session_token"], 1);
    assert_eq!(revoked_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(revoked_payload["provider_counts"]["openai"], 1);
    assert_eq!(revoked_payload["revoked_counts_total"]["revoked"], 1);
    assert_eq!(revoked_payload["revoked_counts"]["revoked"], 1);
    assert_eq!(revoked_payload["entries"][0]["revoked"], true);

    let zero_row_output = execute_auth_command(
        &config,
        "matrix openai --mode session-token --mode-support supported --availability unavailable --state revoked --source-kind credential-store --revoked not-revoked --json",
    );
    let zero_row_payload: serde_json::Value =
        serde_json::from_str(&zero_row_output).expect("parse zero-row revoked composed payload");
    assert_eq!(zero_row_payload["revoked_filter"], "not_revoked");
    assert_eq!(zero_row_payload["rows_total"], 1);
    assert_eq!(zero_row_payload["rows"], 0);
    assert_eq!(zero_row_payload["mode_counts_total"]["session_token"], 1);
    assert_eq!(
        zero_row_payload["mode_counts"]
            .as_object()
            .expect("zero-row revoked mode counts")
            .len(),
        0
    );
    assert_eq!(zero_row_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(
        zero_row_payload["provider_counts"]
            .as_object()
            .expect("zero-row revoked provider counts")
            .len(),
        0
    );
    assert_eq!(zero_row_payload["revoked_counts_total"]["revoked"], 1);
    assert_eq!(
        zero_row_payload["revoked_counts"]
            .as_object()
            .expect("zero-row revoked counts")
            .len(),
        0
    );
    assert_eq!(
        zero_row_payload["entries"]
            .as_array()
            .expect("zero-row revoked entries")
            .len(),
        0
    );
}

#[test]
fn regression_execute_auth_command_matrix_rejects_missing_and_duplicate_mode_support_flags() {
    let config = test_auth_command_config();

    let missing_mode_support = execute_auth_command(&config, "matrix --mode-support");
    assert!(missing_mode_support
        .contains("auth error: missing mode-support filter after --mode-support"));
    assert!(missing_mode_support.contains("usage: /auth matrix"));

    let duplicate_mode_support = execute_auth_command(
        &config,
        "matrix --mode-support all --mode-support supported",
    );
    assert!(duplicate_mode_support.contains("auth error: duplicate --mode-support flag"));
    assert!(duplicate_mode_support.contains("usage: /auth matrix"));

    let missing_source_kind = execute_auth_command(&config, "matrix --source-kind");
    assert!(
        missing_source_kind.contains("auth error: missing source-kind filter after --source-kind")
    );
    assert!(missing_source_kind.contains("usage: /auth matrix"));

    let duplicate_source_kind =
        execute_auth_command(&config, "matrix --source-kind all --source-kind env");
    assert!(duplicate_source_kind.contains("auth error: duplicate --source-kind flag"));
    assert!(duplicate_source_kind.contains("usage: /auth matrix"));

    let missing_revoked = execute_auth_command(&config, "matrix --revoked");
    assert!(missing_revoked.contains("auth error: missing revoked filter after --revoked"));
    assert!(missing_revoked.contains("usage: /auth matrix"));

    let duplicate_revoked = execute_auth_command(&config, "matrix --revoked all --revoked revoked");
    assert!(duplicate_revoked.contains("auth error: duplicate --revoked flag"));
    assert!(duplicate_revoked.contains("usage: /auth matrix"));
}

#[test]
fn integration_auth_conformance_store_backed_status_matrix_handles_stale_token_scenarios() {
    #[derive(Debug)]
    struct StaleCase {
        mode: ProviderAuthMethod,
        record: ProviderCredentialStoreRecord,
        expected_state: &'static str,
        expected_refreshable: bool,
        access_secret: &'static str,
        refresh_secret: Option<&'static str>,
    }

    let temp = tempdir().expect("tempdir");
    let now = current_unix_timestamp();
    let cases = vec![
        StaleCase {
            mode: ProviderAuthMethod::OauthToken,
            record: ProviderCredentialStoreRecord {
                auth_method: ProviderAuthMethod::OauthToken,
                access_token: Some("oauth-access-secret".to_string()),
                refresh_token: Some("oauth-refresh-secret".to_string()),
                expires_unix: Some(now.saturating_sub(1)),
                revoked: false,
            },
            expected_state: "expired_refresh_pending",
            expected_refreshable: true,
            access_secret: "oauth-access-secret",
            refresh_secret: Some("oauth-refresh-secret"),
        },
        StaleCase {
            mode: ProviderAuthMethod::SessionToken,
            record: ProviderCredentialStoreRecord {
                auth_method: ProviderAuthMethod::SessionToken,
                access_token: Some("session-access-secret".to_string()),
                refresh_token: None,
                expires_unix: Some(now.saturating_sub(1)),
                revoked: false,
            },
            expected_state: "expired",
            expected_refreshable: false,
            access_secret: "session-access-secret",
            refresh_secret: None,
        },
        StaleCase {
            mode: ProviderAuthMethod::SessionToken,
            record: ProviderCredentialStoreRecord {
                auth_method: ProviderAuthMethod::SessionToken,
                access_token: Some("revoked-access-secret".to_string()),
                refresh_token: Some("revoked-refresh-secret".to_string()),
                expires_unix: Some(now.saturating_add(60)),
                revoked: true,
            },
            expected_state: "revoked",
            expected_refreshable: false,
            access_secret: "revoked-access-secret",
            refresh_secret: Some("revoked-refresh-secret"),
        },
        StaleCase {
            mode: ProviderAuthMethod::OauthToken,
            record: ProviderCredentialStoreRecord {
                auth_method: ProviderAuthMethod::OauthToken,
                access_token: None,
                refresh_token: Some("missing-access-refresh-secret".to_string()),
                expires_unix: Some(now.saturating_add(60)),
                revoked: false,
            },
            expected_state: "missing_access_token",
            expected_refreshable: true,
            access_secret: "not-present-access-secret",
            refresh_secret: Some("missing-access-refresh-secret"),
        },
    ];

    for (index, case) in cases.into_iter().enumerate() {
        let mut config = test_auth_command_config();
        config.credential_store = temp.path().join(format!("auth-stale-{index}.json"));
        config.credential_store_encryption = CredentialStoreEncryptionMode::None;
        config.api_key = None;
        config.openai_api_key = None;
        set_provider_auth_mode(&mut config, Provider::OpenAi, case.mode);
        write_test_provider_credential(
            &config.credential_store,
            CredentialStoreEncryptionMode::None,
            None,
            Provider::OpenAi,
            case.record,
        );

        let json_output = execute_auth_command(&config, "status openai --json");
        let payload: serde_json::Value =
            serde_json::from_str(&json_output).expect("parse status json");
        let row = &payload["entries"][0];
        assert_eq!(row["provider"], "openai");
        assert_eq!(row["mode"], case.mode.as_str());
        assert_eq!(row["state"], case.expected_state);
        assert_eq!(row["available"], false);
        assert_eq!(row["refreshable"], case.expected_refreshable);
        assert!(!json_output.contains(case.access_secret));
        if let Some(refresh_secret) = case.refresh_secret {
            assert!(!json_output.contains(refresh_secret));
        }

        let text_output = execute_auth_command(&config, "status openai");
        assert!(!text_output.contains(case.access_secret));
        if let Some(refresh_secret) = case.refresh_secret {
            assert!(!text_output.contains(refresh_secret));
        }
    }
}

#[test]
fn integration_execute_auth_command_matrix_reports_store_error_for_supported_non_api_modes() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("broken-auth-store.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    std::fs::write(&config.credential_store, "{not-json").expect("write broken store");

    let output = execute_auth_command(&config, "matrix --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse matrix payload");
    let entries = payload["entries"].as_array().expect("matrix entries");
    let openai_oauth = entries
        .iter()
        .find(|row| row["provider"] == "openai" && row["mode"] == "oauth_token")
        .expect("openai oauth row");
    assert_eq!(openai_oauth["mode_supported"], true);
    assert_eq!(openai_oauth["available"], false);
    assert_eq!(openai_oauth["state"], "store_error");
    assert!(openai_oauth["reason"]
        .as_str()
        .unwrap_or("missing reason")
        .contains("failed to parse"));

    let anthropic_oauth = entries
        .iter()
        .find(|row| row["provider"] == "anthropic" && row["mode"] == "oauth_token")
        .expect("anthropic oauth row");
    assert_eq!(anthropic_oauth["mode_supported"], true);
    assert_eq!(anthropic_oauth["available"], true);
    assert_eq!(anthropic_oauth["state"], "ready");
    assert_eq!(anthropic_oauth["source"], "claude_cli");
}

#[test]
fn regression_execute_auth_command_matrix_rejects_invalid_filter_combinations() {
    let config = test_auth_command_config();

    let missing_mode = execute_auth_command(&config, "matrix --mode");
    assert!(missing_mode.contains("auth error:"));
    assert!(missing_mode.contains("usage: /auth matrix"));

    let duplicate_provider = execute_auth_command(&config, "matrix openai anthropic");
    assert!(duplicate_provider.contains("auth error:"));
    assert!(duplicate_provider.contains("usage: /auth matrix"));

    let duplicate_availability = execute_auth_command(
        &config,
        "matrix --availability available --availability unavailable",
    );
    assert!(duplicate_availability.contains("auth error:"));
    assert!(duplicate_availability.contains("usage: /auth matrix"));

    let missing_state = execute_auth_command(&config, "matrix --state");
    assert!(missing_state.contains("auth error:"));
    assert!(missing_state.contains("usage: /auth matrix"));

    let duplicate_state = execute_auth_command(&config, "matrix --state ready --state revoked");
    assert!(duplicate_state.contains("auth error:"));
    assert!(duplicate_state.contains("usage: /auth matrix"));
}

#[test]
fn regression_auth_security_matrix_blocks_unsupported_mode_bypass_attempts() {
    let unsupported_cases = vec![
        (Provider::OpenAi, ProviderAuthMethod::Adc),
        (Provider::Anthropic, ProviderAuthMethod::Adc),
        (Provider::Google, ProviderAuthMethod::SessionToken),
    ];

    for (provider, mode) in unsupported_cases {
        let capability = provider_auth_capability(provider, mode);
        assert!(!capability.supported);

        let output = execute_auth_command(
            &test_auth_command_config(),
            &format!(
                "login {} --mode {} --json",
                provider.as_str(),
                mode.as_str()
            ),
        );
        let payload: serde_json::Value = serde_json::from_str(&output).expect("parse login output");
        assert_eq!(payload["command"], "auth.login");
        assert_eq!(payload["provider"], provider.as_str());
        assert_eq!(payload["mode"], mode.as_str());
        assert_eq!(payload["status"], "error");
        assert!(payload["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("not supported"));
    }
}

#[test]
fn functional_execute_auth_command_login_status_logout_lifecycle() {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("credentials.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.openai_auth_mode = ProviderAuthMethod::OauthToken;

    let expires_unix = current_unix_timestamp().saturating_add(3600);
    let snapshot = snapshot_env_vars(&[
        "OPENAI_ACCESS_TOKEN",
        "OPENAI_REFRESH_TOKEN",
        "OPENAI_AUTH_EXPIRES_UNIX",
    ]);
    std::env::set_var("OPENAI_ACCESS_TOKEN", "openai-access-token");
    std::env::set_var("OPENAI_REFRESH_TOKEN", "openai-refresh-token");
    std::env::set_var("OPENAI_AUTH_EXPIRES_UNIX", expires_unix.to_string());

    let login_output = execute_auth_command(&config, "login openai --json");
    let login_json: serde_json::Value =
        serde_json::from_str(&login_output).expect("parse login output");
    assert_eq!(login_json["status"], "saved");
    assert_eq!(login_json["provider"], "openai");
    assert_eq!(login_json["mode"], "oauth_token");
    assert_eq!(login_json["expires_unix"], expires_unix);
    assert!(!login_output.contains("openai-access-token"));
    assert!(!login_output.contains("openai-refresh-token"));

    let status_output = execute_auth_command(&config, "status openai --json");
    let status_json: serde_json::Value =
        serde_json::from_str(&status_output).expect("parse status output");
    assert_eq!(status_json["available"], 1);
    assert_eq!(status_json["entries"][0]["provider"], "openai");
    assert_eq!(status_json["entries"][0]["state"], "ready");
    assert_eq!(status_json["entries"][0]["source"], "credential_store");
    assert!(!status_output.contains("openai-access-token"));
    assert!(!status_output.contains("openai-refresh-token"));

    let logout_output = execute_auth_command(&config, "logout openai --json");
    let logout_json: serde_json::Value =
        serde_json::from_str(&logout_output).expect("parse logout output");
    assert_eq!(logout_json["status"], "revoked");

    let post_logout_status = execute_auth_command(&config, "status openai --json");
    let post_logout_json: serde_json::Value =
        serde_json::from_str(&post_logout_status).expect("parse post logout status");
    assert_eq!(post_logout_json["entries"][0]["state"], "revoked");
    assert_eq!(post_logout_json["entries"][0]["available"], false);

    restore_env_vars(snapshot);
}

#[test]
fn unit_execute_auth_command_status_marks_expired_env_access_token_unavailable() {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("env-expired-credentials.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.openai_auth_mode = ProviderAuthMethod::OauthToken;

    let snapshot = snapshot_env_vars(&[
        "TAU_AUTH_ACCESS_TOKEN",
        "TAU_AUTH_EXPIRES_UNIX",
        "OPENAI_ACCESS_TOKEN",
        "OPENAI_AUTH_EXPIRES_UNIX",
    ]);
    std::env::remove_var("TAU_AUTH_ACCESS_TOKEN");
    std::env::remove_var("TAU_AUTH_EXPIRES_UNIX");
    std::env::set_var("OPENAI_ACCESS_TOKEN", "openai-expired-env-access");
    std::env::set_var(
        "OPENAI_AUTH_EXPIRES_UNIX",
        current_unix_timestamp().saturating_sub(5).to_string(),
    );

    let output = execute_auth_command(&config, "status openai --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse status payload");
    assert_eq!(payload["command"], "auth.status");
    assert_eq!(payload["available"], 0);
    assert_eq!(payload["unavailable"], 1);
    assert_eq!(payload["entries"][0]["provider"], "openai");
    assert_eq!(payload["entries"][0]["mode"], "oauth_token");
    assert_eq!(payload["entries"][0]["state"], "expired_env_access_token");
    assert_eq!(payload["entries"][0]["available"], false);
    assert_eq!(payload["entries"][0]["source"], "OPENAI_ACCESS_TOKEN");
    assert!(!output.contains("openai-expired-env-access"));

    restore_env_vars(snapshot);
}

#[test]
fn functional_execute_auth_command_status_uses_env_access_token_when_store_entry_missing() {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("env-fallback-credentials.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.openai_auth_mode = ProviderAuthMethod::OauthToken;

    let snapshot = snapshot_env_vars(&[
        "TAU_AUTH_ACCESS_TOKEN",
        "TAU_AUTH_EXPIRES_UNIX",
        "OPENAI_ACCESS_TOKEN",
        "OPENAI_AUTH_EXPIRES_UNIX",
    ]);
    std::env::remove_var("TAU_AUTH_ACCESS_TOKEN");
    std::env::remove_var("TAU_AUTH_EXPIRES_UNIX");
    std::env::set_var("OPENAI_ACCESS_TOKEN", "openai-env-fallback-access");
    std::env::set_var(
        "OPENAI_AUTH_EXPIRES_UNIX",
        current_unix_timestamp().saturating_add(300).to_string(),
    );

    let output = execute_auth_command(&config, "status openai --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse status payload");
    assert_eq!(payload["command"], "auth.status");
    assert_eq!(payload["available"], 1);
    assert_eq!(payload["unavailable"], 0);
    assert_eq!(payload["source_kind_counts_total"]["env"], 1);
    assert_eq!(payload["source_kind_counts"]["env"], 1);
    assert_eq!(payload["entries"][0]["provider"], "openai");
    assert_eq!(payload["entries"][0]["mode"], "oauth_token");
    assert_eq!(payload["entries"][0]["state"], "ready");
    assert_eq!(payload["entries"][0]["available"], true);
    assert_eq!(payload["entries"][0]["source"], "OPENAI_ACCESS_TOKEN");
    assert_eq!(
        payload["entries"][0]["reason"],
        "env_access_token_available"
    );
    assert!(!output.contains("openai-env-fallback-access"));

    let text_output = execute_auth_command(&config, "status openai");
    assert!(text_output.contains("source=OPENAI_ACCESS_TOKEN"));
    assert!(text_output.contains("source_kind_counts=env:1"));
    assert!(text_output.contains("source_kind_counts_total=env:1"));
    assert!(!text_output.contains("openai-env-fallback-access"));

    restore_env_vars(snapshot);
}

#[test]
fn integration_build_provider_client_supports_openai_oauth_from_env_when_store_entry_missing() {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.openai_auth_mode = CliProviderAuthMode::OauthToken;
    cli.credential_store = temp.path().join("missing-store-entry.json");
    cli.credential_store_encryption = CliCredentialStoreEncryptionMode::None;

    let snapshot = snapshot_env_vars(&[
        "TAU_AUTH_ACCESS_TOKEN",
        "TAU_AUTH_EXPIRES_UNIX",
        "OPENAI_ACCESS_TOKEN",
        "OPENAI_AUTH_EXPIRES_UNIX",
    ]);
    std::env::remove_var("TAU_AUTH_ACCESS_TOKEN");
    std::env::remove_var("TAU_AUTH_EXPIRES_UNIX");
    std::env::set_var("OPENAI_ACCESS_TOKEN", "openai-env-client-access");
    std::env::set_var(
        "OPENAI_AUTH_EXPIRES_UNIX",
        current_unix_timestamp().saturating_add(300).to_string(),
    );

    let client = build_provider_client(&cli, Provider::OpenAi).expect("build env oauth client");
    let ptr = Arc::as_ptr(&client);
    assert!(!ptr.is_null());

    restore_env_vars(snapshot);
}

#[cfg(unix)]
#[test]
fn integration_build_provider_client_uses_codex_backend_when_oauth_store_entry_missing() {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let temp = tempdir().expect("tempdir");
    let script = write_mock_codex_script(
        temp.path(),
        r#"
out=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --output-last-message) out="$2"; shift 2;;
    *) shift;;
  esac
done
cat >/dev/null
printf "codex fallback response" > "$out"
"#,
    );

    let mut cli = test_cli();
    cli.openai_auth_mode = CliProviderAuthMode::OauthToken;
    cli.credential_store = temp.path().join("missing-store-entry.json");
    cli.credential_store_encryption = CliCredentialStoreEncryptionMode::None;
    cli.openai_codex_backend = true;
    cli.openai_codex_cli = script.display().to_string();

    let snapshot = snapshot_env_vars(&[
        "TAU_AUTH_ACCESS_TOKEN",
        "TAU_AUTH_EXPIRES_UNIX",
        "OPENAI_ACCESS_TOKEN",
        "OPENAI_AUTH_EXPIRES_UNIX",
    ]);
    std::env::remove_var("TAU_AUTH_ACCESS_TOKEN");
    std::env::remove_var("TAU_AUTH_EXPIRES_UNIX");
    std::env::remove_var("OPENAI_ACCESS_TOKEN");
    std::env::remove_var("OPENAI_AUTH_EXPIRES_UNIX");

    let client = build_provider_client(&cli, Provider::OpenAi).expect("build codex backend client");
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let response = runtime
        .block_on(client.complete(test_chat_request()))
        .expect("codex backend completion");
    assert_eq!(response.message.text_content(), "codex fallback response");

    restore_env_vars(snapshot);
}

#[test]
fn regression_build_provider_client_does_not_bypass_revoked_store_with_env_token() {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("revoked-store.json");
    write_test_provider_credential(
        &store_path,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("revoked-store-access".to_string()),
            refresh_token: Some("revoked-store-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(300)),
            revoked: true,
        },
    );

    let mut cli = test_cli();
    cli.openai_auth_mode = CliProviderAuthMode::OauthToken;
    cli.credential_store = store_path;
    cli.credential_store_encryption = CliCredentialStoreEncryptionMode::None;

    let snapshot = snapshot_env_vars(&[
        "TAU_AUTH_ACCESS_TOKEN",
        "TAU_AUTH_EXPIRES_UNIX",
        "OPENAI_ACCESS_TOKEN",
        "OPENAI_AUTH_EXPIRES_UNIX",
    ]);
    std::env::remove_var("TAU_AUTH_ACCESS_TOKEN");
    std::env::remove_var("TAU_AUTH_EXPIRES_UNIX");
    std::env::set_var("OPENAI_ACCESS_TOKEN", "openai-env-should-not-bypass");
    std::env::set_var(
        "OPENAI_AUTH_EXPIRES_UNIX",
        current_unix_timestamp().saturating_add(300).to_string(),
    );

    let error = match build_provider_client(&cli, Provider::OpenAi) {
        Ok(_) => panic!("revoked store should remain fail-closed"),
        Err(error) => error,
    };
    let message = error.to_string();
    assert!(message.contains("requires re-authentication"));
    assert!(message.contains("revoked"));
    assert!(!message.contains("openai-env-should-not-bypass"));
    assert!(!message.contains("revoked-store-access"));

    restore_env_vars(snapshot);
}

#[test]
fn integration_execute_auth_command_status_reports_store_backed_state() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("credentials.json");
    write_test_provider_credential(
        &store_path,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::SessionToken,
            access_token: Some("session-access".to_string()),
            refresh_token: Some("session-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(1200)),
            revoked: false,
        },
    );

    let mut config = test_auth_command_config();
    config.credential_store = store_path;
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.openai_auth_mode = ProviderAuthMethod::SessionToken;

    let output = execute_auth_command(&config, "status openai --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse status");
    assert_eq!(payload["provider_filter"], "openai");
    assert_eq!(payload["mode_filter"], "all");
    assert_eq!(payload["entries"][0]["provider"], "openai");
    assert_eq!(payload["entries"][0]["mode"], "session_token");
    assert_eq!(payload["entries"][0]["state"], "ready");
    assert_eq!(payload["entries"][0]["available"], true);
    assert_eq!(payload["mode_supported"], 1);
    assert_eq!(payload["mode_unsupported"], 0);
    assert_eq!(payload["mode_supported_total"], 1);
    assert_eq!(payload["mode_unsupported_total"], 0);
    assert_eq!(payload["provider_counts_total"]["openai"], 1);
    assert_eq!(payload["provider_counts"]["openai"], 1);
    assert_eq!(payload["state_counts"]["ready"], 1);
    assert_eq!(payload["state_counts_total"]["ready"], 1);
    assert_eq!(payload["source_kind_counts_total"]["credential_store"], 1);
    assert_eq!(payload["source_kind_counts"]["credential_store"], 1);
}

#[test]
fn functional_execute_auth_command_status_supports_availability_and_state_filters() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-status-filters.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = None;
    config.openai_api_key = Some("openai-status-filter-key".to_string());
    config.anthropic_api_key = Some("anthropic-status-filter-key".to_string());
    config.google_api_key = None;

    let available_output = execute_auth_command(&config, "status --availability available --json");
    let available_payload: serde_json::Value =
        serde_json::from_str(&available_output).expect("parse available status payload");
    assert_eq!(available_payload["command"], "auth.status");
    assert_eq!(available_payload["provider_filter"], "all");
    assert_eq!(available_payload["mode_filter"], "all");
    assert_eq!(available_payload["availability_filter"], "available");
    assert_eq!(available_payload["state_filter"], "all");
    assert_eq!(available_payload["source_kind_filter"], "all");
    assert_eq!(available_payload["revoked_filter"], "all");
    assert_eq!(available_payload["providers"], 3);
    assert_eq!(available_payload["rows_total"], 3);
    assert_eq!(available_payload["rows"], 2);
    assert_eq!(available_payload["mode_supported_total"], 3);
    assert_eq!(available_payload["mode_unsupported_total"], 0);
    assert_eq!(available_payload["mode_counts_total"]["api_key"], 3);
    assert_eq!(available_payload["mode_counts"]["api_key"], 2);
    assert_eq!(available_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(available_payload["provider_counts_total"]["anthropic"], 1);
    assert_eq!(available_payload["provider_counts_total"]["google"], 1);
    assert_eq!(available_payload["provider_counts"]["openai"], 1);
    assert_eq!(available_payload["provider_counts"]["anthropic"], 1);
    assert_eq!(
        available_payload["availability_counts_total"]["available"],
        2
    );
    assert_eq!(
        available_payload["availability_counts_total"]["unavailable"],
        1
    );
    assert_eq!(available_payload["availability_counts"]["available"], 2);
    assert_eq!(available_payload["available"], 2);
    assert_eq!(available_payload["unavailable"], 0);
    assert_eq!(available_payload["source_kind_counts_total"]["flag"], 2);
    assert_eq!(available_payload["source_kind_counts_total"]["none"], 1);
    assert_eq!(available_payload["source_kind_counts"]["flag"], 2);
    assert_eq!(available_payload["revoked_counts_total"]["not_revoked"], 3);
    assert_eq!(available_payload["revoked_counts"]["not_revoked"], 2);
    assert_eq!(available_payload["state_counts"]["ready"], 2);
    assert_eq!(available_payload["state_counts_total"]["ready"], 2);
    assert_eq!(
        available_payload["state_counts_total"]["missing_api_key"],
        1
    );
    let available_entries = available_payload["entries"]
        .as_array()
        .expect("available status entries");
    assert_eq!(available_entries.len(), 2);
    assert!(available_entries
        .iter()
        .all(|entry| entry["available"].as_bool() == Some(true)));
    assert!(available_entries
        .iter()
        .all(|entry| entry["state"] == "ready"));

    let unavailable_output =
        execute_auth_command(&config, "status --availability unavailable --json");
    let unavailable_payload: serde_json::Value =
        serde_json::from_str(&unavailable_output).expect("parse unavailable status payload");
    assert_eq!(unavailable_payload["provider_filter"], "all");
    assert_eq!(unavailable_payload["availability_filter"], "unavailable");
    assert_eq!(unavailable_payload["mode_filter"], "all");
    assert_eq!(unavailable_payload["state_filter"], "all");
    assert_eq!(unavailable_payload["source_kind_filter"], "all");
    assert_eq!(unavailable_payload["revoked_filter"], "all");
    assert_eq!(unavailable_payload["providers"], 3);
    assert_eq!(unavailable_payload["rows_total"], 3);
    assert_eq!(unavailable_payload["rows"], 1);
    assert_eq!(unavailable_payload["mode_supported_total"], 3);
    assert_eq!(unavailable_payload["mode_unsupported_total"], 0);
    assert_eq!(unavailable_payload["mode_counts_total"]["api_key"], 3);
    assert_eq!(unavailable_payload["mode_counts"]["api_key"], 1);
    assert_eq!(unavailable_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(unavailable_payload["provider_counts_total"]["anthropic"], 1);
    assert_eq!(unavailable_payload["provider_counts_total"]["google"], 1);
    assert_eq!(unavailable_payload["provider_counts"]["google"], 1);
    assert_eq!(
        unavailable_payload["availability_counts_total"]["available"],
        2
    );
    assert_eq!(
        unavailable_payload["availability_counts_total"]["unavailable"],
        1
    );
    assert_eq!(unavailable_payload["availability_counts"]["unavailable"], 1);
    assert_eq!(unavailable_payload["available"], 0);
    assert_eq!(unavailable_payload["unavailable"], 1);
    assert_eq!(unavailable_payload["source_kind_counts_total"]["flag"], 2);
    assert_eq!(unavailable_payload["source_kind_counts_total"]["none"], 1);
    assert_eq!(unavailable_payload["source_kind_counts"]["none"], 1);
    assert_eq!(
        unavailable_payload["revoked_counts_total"]["not_revoked"],
        3
    );
    assert_eq!(unavailable_payload["revoked_counts"]["not_revoked"], 1);
    assert_eq!(unavailable_payload["state_counts"]["missing_api_key"], 1);
    assert_eq!(unavailable_payload["state_counts_total"]["ready"], 2);
    assert_eq!(
        unavailable_payload["state_counts_total"]["missing_api_key"],
        1
    );
    assert_eq!(unavailable_payload["entries"][0]["provider"], "google");
    assert_eq!(
        unavailable_payload["entries"][0]["state"],
        "missing_api_key"
    );
    assert_eq!(unavailable_payload["entries"][0]["available"], false);

    let state_output = execute_auth_command(&config, "status --state missing_api_key --json");
    let state_payload: serde_json::Value =
        serde_json::from_str(&state_output).expect("parse state-filtered status payload");
    assert_eq!(state_payload["provider_filter"], "all");
    assert_eq!(state_payload["availability_filter"], "all");
    assert_eq!(state_payload["mode_filter"], "all");
    assert_eq!(state_payload["state_filter"], "missing_api_key");
    assert_eq!(state_payload["source_kind_filter"], "all");
    assert_eq!(state_payload["revoked_filter"], "all");
    assert_eq!(state_payload["providers"], 3);
    assert_eq!(state_payload["rows_total"], 3);
    assert_eq!(state_payload["rows"], 1);
    assert_eq!(state_payload["mode_supported_total"], 3);
    assert_eq!(state_payload["mode_unsupported_total"], 0);
    assert_eq!(state_payload["mode_counts_total"]["api_key"], 3);
    assert_eq!(state_payload["mode_counts"]["api_key"], 1);
    assert_eq!(state_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(state_payload["provider_counts_total"]["anthropic"], 1);
    assert_eq!(state_payload["provider_counts_total"]["google"], 1);
    assert_eq!(state_payload["provider_counts"]["google"], 1);
    assert_eq!(state_payload["availability_counts_total"]["available"], 2);
    assert_eq!(state_payload["availability_counts_total"]["unavailable"], 1);
    assert_eq!(state_payload["availability_counts"]["unavailable"], 1);
    assert_eq!(state_payload["state_counts"]["missing_api_key"], 1);
    assert_eq!(state_payload["source_kind_counts_total"]["flag"], 2);
    assert_eq!(state_payload["source_kind_counts_total"]["none"], 1);
    assert_eq!(state_payload["source_kind_counts"]["none"], 1);
    assert_eq!(state_payload["revoked_counts_total"]["not_revoked"], 3);
    assert_eq!(state_payload["revoked_counts"]["not_revoked"], 1);
    assert_eq!(state_payload["state_counts_total"]["ready"], 2);
    assert_eq!(state_payload["state_counts_total"]["missing_api_key"], 1);
    assert_eq!(state_payload["entries"][0]["provider"], "google");
    assert_eq!(state_payload["entries"][0]["state"], "missing_api_key");

    let text_output = execute_auth_command(
        &config,
        "status --availability unavailable --state missing_api_key",
    );
    assert!(text_output.contains("provider_filter=all"));
    assert!(text_output.contains("mode_supported_total=3"));
    assert!(text_output.contains("mode_unsupported_total=0"));
    assert!(text_output.contains("mode_counts=api_key:1"));
    assert!(text_output.contains("mode_counts_total=api_key:3"));
    assert!(text_output.contains("provider_counts=google:1"));
    assert!(text_output.contains("provider_counts_total=anthropic:1,google:1,openai:1"));
    assert!(text_output.contains("availability_counts=unavailable:1"));
    assert!(text_output.contains("availability_counts_total=available:2,unavailable:1"));
    assert!(text_output.contains("source_kind_counts=none:1"));
    assert!(text_output.contains("source_kind_counts_total=flag:2,none:1"));
    assert!(text_output.contains("revoked_counts=not_revoked:1"));
    assert!(text_output.contains("revoked_counts_total=not_revoked:3"));
    assert!(text_output.contains("availability_filter=unavailable"));
    assert!(text_output.contains("state_filter=missing_api_key"));
    assert!(text_output.contains("source_kind_filter=all"));
    assert!(text_output.contains("revoked_filter=all"));
    assert!(text_output.contains("rows_total=3"));
    assert!(text_output.contains("state_counts=missing_api_key:1"));
    assert!(text_output.contains("state_counts_total=missing_api_key:1,ready:2"));
    assert!(text_output.contains("auth provider: name=google"));
    assert!(!text_output.contains("auth provider: name=openai"));
}

#[test]
fn functional_execute_auth_command_status_supports_mode_filter() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-status-mode-filter.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = None;
    config.openai_api_key = Some("openai-mode-filter-key".to_string());
    config.anthropic_api_key = None;
    config.google_api_key = None;

    let api_key_output = execute_auth_command(&config, "status --mode api-key --json");
    let api_key_payload: serde_json::Value =
        serde_json::from_str(&api_key_output).expect("parse api-key mode-filtered status payload");
    assert_eq!(api_key_payload["command"], "auth.status");
    assert_eq!(api_key_payload["provider_filter"], "all");
    assert_eq!(api_key_payload["mode_filter"], "api_key");
    assert_eq!(api_key_payload["mode_support_filter"], "all");
    assert_eq!(api_key_payload["source_kind_filter"], "all");
    assert_eq!(api_key_payload["revoked_filter"], "all");
    assert_eq!(api_key_payload["providers"], 3);
    assert_eq!(api_key_payload["rows_total"], 3);
    assert_eq!(api_key_payload["rows"], 3);
    assert_eq!(api_key_payload["mode_counts_total"]["api_key"], 3);
    assert_eq!(api_key_payload["mode_counts"]["api_key"], 3);
    assert_eq!(api_key_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(api_key_payload["provider_counts_total"]["anthropic"], 1);
    assert_eq!(api_key_payload["provider_counts_total"]["google"], 1);
    assert_eq!(api_key_payload["provider_counts"]["openai"], 1);
    assert_eq!(api_key_payload["provider_counts"]["anthropic"], 1);
    assert_eq!(api_key_payload["provider_counts"]["google"], 1);
    assert_eq!(
        api_key_payload["mode_supported_total"]
            .as_u64()
            .unwrap_or(0)
            + api_key_payload["mode_unsupported_total"]
                .as_u64()
                .unwrap_or(0),
        3
    );
    assert_eq!(api_key_payload["source_kind_counts_total"]["flag"], 1);
    assert_eq!(api_key_payload["source_kind_counts"]["flag"], 1);
    assert_eq!(
        api_key_payload["source_kind_counts_total"]
            .as_object()
            .expect("api-key source-kind counts total")
            .values()
            .map(|value| value.as_u64().unwrap_or(0))
            .sum::<u64>(),
        3
    );
    assert_eq!(
        api_key_payload["source_kind_counts"]
            .as_object()
            .expect("api-key source-kind counts")
            .values()
            .map(|value| value.as_u64().unwrap_or(0))
            .sum::<u64>(),
        3
    );
    assert_eq!(
        api_key_payload["available"].as_u64().unwrap_or(0)
            + api_key_payload["unavailable"].as_u64().unwrap_or(0),
        3
    );
    let api_key_entries = api_key_payload["entries"]
        .as_array()
        .expect("api-key mode-filtered status entries");
    assert_eq!(api_key_entries.len(), 3);
    assert!(api_key_entries
        .iter()
        .all(|entry| entry["mode"] == "api_key"));
    let openai_entry = api_key_entries
        .iter()
        .find(|entry| entry["provider"] == "openai")
        .expect("openai status row");
    assert_eq!(openai_entry["available"], true);
    assert_eq!(openai_entry["state"], "ready");

    let oauth_output = execute_auth_command(&config, "status --mode oauth-token --json");
    let oauth_payload: serde_json::Value =
        serde_json::from_str(&oauth_output).expect("parse oauth mode-filtered status payload");
    assert_eq!(oauth_payload["provider_filter"], "all");
    assert_eq!(oauth_payload["mode_filter"], "oauth_token");
    assert_eq!(oauth_payload["source_kind_filter"], "all");
    assert_eq!(oauth_payload["revoked_filter"], "all");
    assert_eq!(oauth_payload["providers"], 3);
    assert_eq!(oauth_payload["rows_total"], 3);
    assert_eq!(oauth_payload["rows"], 3);
    assert_eq!(oauth_payload["mode_counts_total"]["oauth_token"], 3);
    assert_eq!(oauth_payload["mode_counts"]["oauth_token"], 3);
    assert_eq!(oauth_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(oauth_payload["provider_counts_total"]["anthropic"], 1);
    assert_eq!(oauth_payload["provider_counts_total"]["google"], 1);
    assert_eq!(oauth_payload["provider_counts"]["openai"], 1);
    assert_eq!(oauth_payload["provider_counts"]["anthropic"], 1);
    assert_eq!(oauth_payload["provider_counts"]["google"], 1);
    assert_eq!(
        oauth_payload["mode_supported_total"].as_u64().unwrap_or(0)
            + oauth_payload["mode_unsupported_total"]
                .as_u64()
                .unwrap_or(0),
        3
    );
    assert_eq!(
        oauth_payload["source_kind_counts_total"]
            .as_object()
            .expect("oauth source-kind counts total")
            .values()
            .map(|value| value.as_u64().unwrap_or(0))
            .sum::<u64>(),
        3
    );
    let oauth_entries = oauth_payload["entries"]
        .as_array()
        .expect("oauth mode-filtered status entries");
    assert_eq!(oauth_entries.len(), 3);
    assert!(oauth_entries
        .iter()
        .all(|entry| entry["mode"] == "oauth_token"));

    let text_output = execute_auth_command(&config, "status --mode api-key");
    assert!(text_output.contains("provider_filter=all"));
    assert!(text_output.contains("mode_filter=api_key"));
    assert!(text_output.contains("source_kind_filter=all"));
    assert!(text_output.contains("revoked_filter=all"));
    assert!(text_output.contains("mode_counts=api_key:3"));
    assert!(text_output.contains("mode_counts_total=api_key:3"));
    assert!(text_output.contains("provider_counts=anthropic:1,google:1,openai:1"));
    assert!(text_output.contains("provider_counts_total=anthropic:1,google:1,openai:1"));
    assert!(text_output.contains("source_kind_counts="));
    assert!(text_output.contains("source_kind_counts_total="));
    assert!(text_output.contains("flag:1"));
    assert!(text_output.contains("auth provider: name=openai mode=api_key"));
}

#[test]
fn functional_execute_auth_command_status_supports_mode_support_filter() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-status-mode-support-filter.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = None;
    config.openai_api_key = Some("openai-mode-support-key".to_string());
    config.google_api_key = None;
    config.anthropic_auth_mode = ProviderAuthMethod::OauthToken;

    let supported_output = execute_auth_command(&config, "status --mode-support supported --json");
    let supported_payload: serde_json::Value =
        serde_json::from_str(&supported_output).expect("parse supported-only status payload");
    assert_eq!(supported_payload["provider_filter"], "all");
    assert_eq!(supported_payload["mode_support_filter"], "supported");
    assert_eq!(supported_payload["mode_filter"], "all");
    assert_eq!(supported_payload["source_kind_filter"], "all");
    assert_eq!(supported_payload["revoked_filter"], "all");
    assert_eq!(supported_payload["providers"], 3);
    assert_eq!(supported_payload["rows_total"], 3);
    assert_eq!(supported_payload["rows"], 3);
    assert_eq!(supported_payload["mode_supported"], 3);
    assert_eq!(supported_payload["mode_unsupported"], 0);
    assert_eq!(supported_payload["mode_supported_total"], 3);
    assert_eq!(supported_payload["mode_unsupported_total"], 0);
    assert_eq!(supported_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(supported_payload["provider_counts_total"]["anthropic"], 1);
    assert_eq!(supported_payload["provider_counts_total"]["google"], 1);
    assert_eq!(supported_payload["provider_counts"]["openai"], 1);
    assert_eq!(supported_payload["provider_counts"]["anthropic"], 1);
    assert_eq!(supported_payload["provider_counts"]["google"], 1);
    assert_eq!(supported_payload["source_kind_counts_total"]["flag"], 1);
    assert_eq!(supported_payload["source_kind_counts_total"]["env"], 1);
    assert_eq!(supported_payload["source_kind_counts_total"]["none"], 1);
    assert_eq!(supported_payload["source_kind_counts"]["flag"], 1);
    assert_eq!(supported_payload["source_kind_counts"]["env"], 1);
    assert_eq!(supported_payload["source_kind_counts"]["none"], 1);
    assert_eq!(
        supported_payload["source_kind_counts_total"]
            .as_object()
            .expect("supported source-kind counts total")
            .values()
            .map(|value| value.as_u64().unwrap_or(0))
            .sum::<u64>(),
        3
    );
    assert_eq!(
        supported_payload["source_kind_counts"]
            .as_object()
            .expect("supported source-kind counts")
            .values()
            .map(|value| value.as_u64().unwrap_or(0))
            .sum::<u64>(),
        3
    );
    assert_eq!(supported_payload["state_counts"]["missing_api_key"], 1);
    assert_eq!(supported_payload["state_counts"]["ready"], 2);
    assert_eq!(
        supported_payload["state_counts_total"]["missing_api_key"],
        1
    );
    assert_eq!(supported_payload["state_counts_total"]["ready"], 2);
    assert!(supported_payload["state_counts_total"]["unsupported_mode"].is_null());
    let supported_entries = supported_payload["entries"]
        .as_array()
        .expect("supported status entries");
    assert_eq!(supported_entries.len(), 3);
    assert!(supported_entries
        .iter()
        .all(|entry| entry["mode_supported"] == true));

    let unsupported_output =
        execute_auth_command(&config, "status --mode-support unsupported --json");
    let unsupported_payload: serde_json::Value =
        serde_json::from_str(&unsupported_output).expect("parse unsupported-only status payload");
    assert_eq!(unsupported_payload["provider_filter"], "all");
    assert_eq!(unsupported_payload["mode_support_filter"], "unsupported");
    assert_eq!(unsupported_payload["mode_filter"], "all");
    assert_eq!(unsupported_payload["source_kind_filter"], "all");
    assert_eq!(unsupported_payload["revoked_filter"], "all");
    assert_eq!(unsupported_payload["rows_total"], 3);
    assert_eq!(unsupported_payload["rows"], 0);
    assert_eq!(unsupported_payload["mode_supported"], 0);
    assert_eq!(unsupported_payload["mode_unsupported"], 0);
    assert_eq!(unsupported_payload["mode_supported_total"], 3);
    assert_eq!(unsupported_payload["mode_unsupported_total"], 0);
    assert_eq!(unsupported_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(unsupported_payload["provider_counts_total"]["anthropic"], 1);
    assert_eq!(unsupported_payload["provider_counts_total"]["google"], 1);
    assert_eq!(unsupported_payload["source_kind_counts_total"]["flag"], 1);
    assert_eq!(unsupported_payload["source_kind_counts_total"]["env"], 1);
    assert_eq!(unsupported_payload["source_kind_counts_total"]["none"], 1);
    assert_eq!(
        unsupported_payload["source_kind_counts"]
            .as_object()
            .expect("unsupported source-kind counts")
            .len(),
        0
    );
    assert_eq!(
        unsupported_payload["source_kind_counts_total"]
            .as_object()
            .expect("unsupported source-kind counts total")
            .values()
            .map(|value| value.as_u64().unwrap_or(0))
            .sum::<u64>(),
        3
    );
    assert_eq!(
        unsupported_payload["state_counts"]
            .as_object()
            .expect("unsupported state counts")
            .len(),
        0
    );
    assert_eq!(
        unsupported_payload["entries"]
            .as_array()
            .expect("unsupported entries")
            .len(),
        0
    );

    let text_output = execute_auth_command(&config, "status --mode-support unsupported");
    assert!(text_output.contains("provider_filter=all"));
    assert!(text_output.contains("mode_support_filter=unsupported"));
    assert!(text_output.contains("source_kind_filter=all"));
    assert!(text_output.contains("revoked_filter=all"));
    assert!(text_output.contains("mode_supported_total=3"));
    assert!(text_output.contains("mode_unsupported_total=0"));
    assert!(text_output.contains("provider_counts=none"));
    assert!(text_output.contains("provider_counts_total=anthropic:1,google:1,openai:1"));
    assert!(text_output.contains("source_kind_counts=none"));
    assert!(text_output.contains("source_kind_counts_total="));
    assert!(text_output.contains("flag:1"));
    assert!(text_output.contains("state_counts=none"));
    assert!(!text_output.contains("auth provider: name=openai"));
}

#[test]
fn functional_execute_auth_command_status_supports_source_kind_filter() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-status-source-kind-filter.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = None;
    config.openai_api_key = Some("openai-source-kind-key".to_string());
    config.anthropic_api_key = Some("anthropic-source-kind-key".to_string());
    config.google_api_key = None;

    let flag_output = execute_auth_command(&config, "status --source-kind flag --json");
    let flag_payload: serde_json::Value =
        serde_json::from_str(&flag_output).expect("parse flag source-kind status payload");
    assert_eq!(flag_payload["provider_filter"], "all");
    assert_eq!(flag_payload["mode_filter"], "all");
    assert_eq!(flag_payload["source_kind_filter"], "flag");
    assert_eq!(flag_payload["revoked_filter"], "all");
    assert_eq!(flag_payload["rows_total"], 3);
    assert_eq!(flag_payload["rows"], 2);
    assert_eq!(flag_payload["available"], 2);
    assert_eq!(flag_payload["unavailable"], 0);
    assert_eq!(flag_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(flag_payload["provider_counts_total"]["anthropic"], 1);
    assert_eq!(flag_payload["provider_counts_total"]["google"], 1);
    assert_eq!(flag_payload["provider_counts"]["openai"], 1);
    assert_eq!(flag_payload["provider_counts"]["anthropic"], 1);
    assert_eq!(flag_payload["source_kind_counts_total"]["flag"], 2);
    assert_eq!(flag_payload["source_kind_counts_total"]["none"], 1);
    assert_eq!(flag_payload["source_kind_counts"]["flag"], 2);
    let flag_entries = flag_payload["entries"]
        .as_array()
        .expect("flag source-kind entries");
    assert_eq!(flag_entries.len(), 2);
    assert!(flag_entries
        .iter()
        .all(|entry| entry["source"].as_str().unwrap_or("").starts_with("--")));

    let none_output = execute_auth_command(&config, "status --source-kind none --json");
    let none_payload: serde_json::Value =
        serde_json::from_str(&none_output).expect("parse none source-kind status payload");
    assert_eq!(none_payload["source_kind_filter"], "none");
    assert_eq!(none_payload["revoked_filter"], "all");
    assert_eq!(none_payload["rows_total"], 3);
    assert_eq!(none_payload["rows"], 1);
    assert_eq!(none_payload["available"], 0);
    assert_eq!(none_payload["unavailable"], 1);
    assert_eq!(none_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(none_payload["provider_counts_total"]["anthropic"], 1);
    assert_eq!(none_payload["provider_counts_total"]["google"], 1);
    assert_eq!(none_payload["provider_counts"]["google"], 1);
    assert_eq!(none_payload["source_kind_counts"]["none"], 1);
    assert_eq!(none_payload["entries"][0]["provider"], "google");
    assert_eq!(none_payload["entries"][0]["state"], "missing_api_key");

    let text_output = execute_auth_command(&config, "status --source-kind none");
    assert!(text_output.contains("source_kind_filter=none"));
    assert!(text_output.contains("revoked_filter=all"));
    assert!(text_output.contains("rows=1"));
    assert!(text_output.contains("provider_counts=google:1"));
    assert!(text_output.contains("provider_counts_total=anthropic:1,google:1,openai:1"));
    assert!(text_output.contains("source_kind_counts=none:1"));
}

#[test]
fn functional_execute_auth_command_status_supports_revoked_filter() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-status-revoked-filter.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.openai_auth_mode = ProviderAuthMethod::SessionToken;

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::SessionToken,
            access_token: Some("status-revoked-access".to_string()),
            refresh_token: Some("status-revoked-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(600)),
            revoked: true,
        },
    );

    let revoked_output = execute_auth_command(&config, "status --revoked revoked --json");
    let revoked_payload: serde_json::Value =
        serde_json::from_str(&revoked_output).expect("parse revoked status payload");
    assert_eq!(revoked_payload["provider_filter"], "all");
    assert_eq!(revoked_payload["mode_filter"], "all");
    assert_eq!(revoked_payload["revoked_filter"], "revoked");
    assert_eq!(revoked_payload["rows_total"], 3);
    assert_eq!(revoked_payload["rows"], 1);
    assert_eq!(revoked_payload["mode_counts_total"]["api_key"], 2);
    assert_eq!(revoked_payload["mode_counts_total"]["session_token"], 1);
    assert_eq!(revoked_payload["mode_counts"]["session_token"], 1);
    assert_eq!(revoked_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(revoked_payload["provider_counts_total"]["anthropic"], 1);
    assert_eq!(revoked_payload["provider_counts_total"]["google"], 1);
    assert_eq!(revoked_payload["provider_counts"]["openai"], 1);
    assert_eq!(revoked_payload["state_counts"]["revoked"], 1);
    assert_eq!(revoked_payload["source_kind_counts"]["credential_store"], 1);
    assert_eq!(revoked_payload["revoked_counts_total"]["not_revoked"], 2);
    assert_eq!(revoked_payload["revoked_counts_total"]["revoked"], 1);
    assert_eq!(revoked_payload["revoked_counts"]["revoked"], 1);
    assert_eq!(revoked_payload["entries"][0]["provider"], "openai");
    assert_eq!(revoked_payload["entries"][0]["revoked"], true);

    let not_revoked_output = execute_auth_command(&config, "status --revoked not-revoked --json");
    let not_revoked_payload: serde_json::Value =
        serde_json::from_str(&not_revoked_output).expect("parse non-revoked status payload");
    assert_eq!(not_revoked_payload["revoked_filter"], "not_revoked");
    assert_eq!(not_revoked_payload["rows_total"], 3);
    assert_eq!(not_revoked_payload["rows"], 2);
    assert_eq!(not_revoked_payload["mode_counts_total"]["api_key"], 2);
    assert_eq!(not_revoked_payload["mode_counts_total"]["session_token"], 1);
    assert_eq!(not_revoked_payload["mode_counts"]["api_key"], 2);
    assert_eq!(not_revoked_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(not_revoked_payload["provider_counts_total"]["anthropic"], 1);
    assert_eq!(not_revoked_payload["provider_counts_total"]["google"], 1);
    assert_eq!(not_revoked_payload["provider_counts"]["anthropic"], 1);
    assert_eq!(not_revoked_payload["provider_counts"]["google"], 1);
    assert_eq!(
        not_revoked_payload["revoked_counts_total"]["not_revoked"],
        2
    );
    assert_eq!(not_revoked_payload["revoked_counts_total"]["revoked"], 1);
    assert_eq!(not_revoked_payload["revoked_counts"]["not_revoked"], 2);
    let not_revoked_entries = not_revoked_payload["entries"]
        .as_array()
        .expect("non-revoked status entries");
    assert_eq!(not_revoked_entries.len(), 2);
    assert!(not_revoked_entries
        .iter()
        .all(|entry| entry["revoked"] == false));

    let text_output = execute_auth_command(&config, "status --revoked revoked");
    assert!(text_output.contains("revoked_filter=revoked"));
    assert!(text_output.contains("rows=1"));
    assert!(text_output.contains("mode_counts=session_token:1"));
    assert!(text_output.contains("mode_counts_total=api_key:2,session_token:1"));
    assert!(text_output.contains("provider_counts=openai:1"));
    assert!(text_output.contains("provider_counts_total=anthropic:1,google:1,openai:1"));
    assert!(text_output.contains("revoked_counts=revoked:1"));
    assert!(text_output.contains("revoked_counts_total=not_revoked:2,revoked:1"));
}

#[test]
fn integration_execute_auth_command_status_filters_compose_with_provider_and_zero_rows() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("auth-status-composition.json");
    write_test_provider_credential(
        &store_path,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::SessionToken,
            access_token: Some("composition-session-access".to_string()),
            refresh_token: Some("composition-session-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(1200)),
            revoked: false,
        },
    );

    let mut config = test_auth_command_config();
    config.credential_store = store_path;
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.openai_auth_mode = ProviderAuthMethod::SessionToken;

    let filtered_output = execute_auth_command(
        &config,
        "status openai --availability available --state ready --source-kind credential-store --json",
    );
    let filtered_payload: serde_json::Value =
        serde_json::from_str(&filtered_output).expect("parse composed status payload");
    assert_eq!(filtered_payload["provider_filter"], "openai");
    assert_eq!(filtered_payload["availability_filter"], "available");
    assert_eq!(filtered_payload["mode_filter"], "all");
    assert_eq!(filtered_payload["state_filter"], "ready");
    assert_eq!(filtered_payload["source_kind_filter"], "credential_store");
    assert_eq!(filtered_payload["revoked_filter"], "all");
    assert_eq!(filtered_payload["providers"], 1);
    assert_eq!(filtered_payload["rows_total"], 1);
    assert_eq!(filtered_payload["rows"], 1);
    assert_eq!(filtered_payload["mode_supported"], 1);
    assert_eq!(filtered_payload["mode_unsupported"], 0);
    assert_eq!(filtered_payload["mode_supported_total"], 1);
    assert_eq!(filtered_payload["mode_unsupported_total"], 0);
    assert_eq!(filtered_payload["mode_counts_total"]["session_token"], 1);
    assert_eq!(filtered_payload["mode_counts"]["session_token"], 1);
    assert_eq!(filtered_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(filtered_payload["provider_counts"]["openai"], 1);
    assert_eq!(
        filtered_payload["availability_counts_total"]["available"],
        1
    );
    assert_eq!(filtered_payload["availability_counts"]["available"], 1);
    assert_eq!(
        filtered_payload["source_kind_counts_total"]["credential_store"],
        1
    );
    assert_eq!(
        filtered_payload["source_kind_counts"]["credential_store"],
        1
    );
    assert_eq!(filtered_payload["available"], 1);
    assert_eq!(filtered_payload["unavailable"], 0);
    assert_eq!(filtered_payload["state_counts"]["ready"], 1);
    assert_eq!(filtered_payload["state_counts_total"]["ready"], 1);
    assert_eq!(filtered_payload["revoked_counts_total"]["not_revoked"], 1);
    assert_eq!(filtered_payload["revoked_counts"]["not_revoked"], 1);
    assert_eq!(filtered_payload["entries"][0]["provider"], "openai");
    assert_eq!(filtered_payload["entries"][0]["state"], "ready");
    assert_eq!(filtered_payload["entries"][0]["available"], true);

    let zero_row_output = execute_auth_command(
        &config,
        "status openai --availability unavailable --state ready --source-kind credential-store --json",
    );
    let zero_row_payload: serde_json::Value =
        serde_json::from_str(&zero_row_output).expect("parse zero-row composed status payload");
    assert_eq!(zero_row_payload["provider_filter"], "openai");
    assert_eq!(zero_row_payload["availability_filter"], "unavailable");
    assert_eq!(zero_row_payload["mode_filter"], "all");
    assert_eq!(zero_row_payload["state_filter"], "ready");
    assert_eq!(zero_row_payload["source_kind_filter"], "credential_store");
    assert_eq!(zero_row_payload["revoked_filter"], "all");
    assert_eq!(zero_row_payload["providers"], 1);
    assert_eq!(zero_row_payload["rows_total"], 1);
    assert_eq!(zero_row_payload["rows"], 0);
    assert_eq!(zero_row_payload["mode_supported"], 0);
    assert_eq!(zero_row_payload["mode_unsupported"], 0);
    assert_eq!(zero_row_payload["mode_supported_total"], 1);
    assert_eq!(zero_row_payload["mode_unsupported_total"], 0);
    assert_eq!(zero_row_payload["mode_counts_total"]["session_token"], 1);
    assert_eq!(
        zero_row_payload["mode_counts"]
            .as_object()
            .expect("zero-row status mode counts")
            .len(),
        0
    );
    assert_eq!(zero_row_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(
        zero_row_payload["provider_counts"]
            .as_object()
            .expect("zero-row status provider counts")
            .len(),
        0
    );
    assert_eq!(
        zero_row_payload["availability_counts_total"]["available"],
        1
    );
    assert_eq!(
        zero_row_payload["availability_counts"]
            .as_object()
            .expect("zero-row availability counts")
            .len(),
        0
    );
    assert_eq!(
        zero_row_payload["source_kind_counts_total"]["credential_store"],
        1
    );
    assert_eq!(
        zero_row_payload["source_kind_counts"]
            .as_object()
            .expect("zero-row source-kind counts")
            .len(),
        0
    );
    assert_eq!(zero_row_payload["available"], 0);
    assert_eq!(zero_row_payload["unavailable"], 0);
    assert_eq!(
        zero_row_payload["state_counts"]
            .as_object()
            .expect("zero-row state counts")
            .len(),
        0
    );
    assert_eq!(zero_row_payload["state_counts_total"]["ready"], 1);
    assert_eq!(zero_row_payload["revoked_counts_total"]["not_revoked"], 1);
    assert_eq!(
        zero_row_payload["revoked_counts"]
            .as_object()
            .expect("zero-row revoked status counts")
            .len(),
        0
    );
    assert_eq!(
        zero_row_payload["entries"]
            .as_array()
            .expect("zero-row entries")
            .len(),
        0
    );

    let zero_row_text = execute_auth_command(
        &config,
        "status openai --availability unavailable --state ready --source-kind credential-store",
    );
    assert!(zero_row_text.contains("providers=1 rows=0"));
    assert!(zero_row_text.contains("provider_filter=openai"));
    assert!(zero_row_text.contains("source_kind_filter=credential_store"));
    assert!(zero_row_text.contains("revoked_filter=all"));
    assert!(zero_row_text.contains("mode_supported_total=1"));
    assert!(zero_row_text.contains("mode_unsupported_total=0"));
    assert!(zero_row_text.contains("mode_counts=none"));
    assert!(zero_row_text.contains("mode_counts_total=session_token:1"));
    assert!(zero_row_text.contains("provider_counts=none"));
    assert!(zero_row_text.contains("provider_counts_total=openai:1"));
    assert!(zero_row_text.contains("availability_counts=none"));
    assert!(zero_row_text.contains("availability_counts_total=available:1"));
    assert!(zero_row_text.contains("source_kind_counts=none"));
    assert!(zero_row_text.contains("source_kind_counts_total=credential_store:1"));
    assert!(zero_row_text.contains("revoked_counts=none"));
    assert!(zero_row_text.contains("revoked_counts_total=not_revoked:1"));
    assert!(zero_row_text.contains("rows_total=1"));
    assert!(zero_row_text.contains("state_counts=none"));
    assert!(zero_row_text.contains("state_counts_total=ready:1"));
}

#[test]
fn integration_execute_auth_command_status_mode_support_filter_composes_with_other_filters() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp
        .path()
        .join("auth-status-mode-support-composition.json");
    write_test_provider_credential(
        &store_path,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::SessionToken,
            access_token: Some("status-mode-support-access".to_string()),
            refresh_token: Some("status-mode-support-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(1200)),
            revoked: false,
        },
    );

    let mut config = test_auth_command_config();
    config.credential_store = store_path;
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.openai_auth_mode = ProviderAuthMethod::SessionToken;

    let filtered_output = execute_auth_command(
        &config,
        "status openai --mode session-token --mode-support supported --availability available --state ready --source-kind credential-store --json",
    );
    let filtered_payload: serde_json::Value =
        serde_json::from_str(&filtered_output).expect("parse composed mode-support payload");
    assert_eq!(filtered_payload["provider_filter"], "openai");
    assert_eq!(filtered_payload["mode_filter"], "session_token");
    assert_eq!(filtered_payload["mode_support_filter"], "supported");
    assert_eq!(filtered_payload["availability_filter"], "available");
    assert_eq!(filtered_payload["state_filter"], "ready");
    assert_eq!(filtered_payload["source_kind_filter"], "credential_store");
    assert_eq!(filtered_payload["revoked_filter"], "all");
    assert_eq!(filtered_payload["providers"], 1);
    assert_eq!(filtered_payload["rows_total"], 1);
    assert_eq!(filtered_payload["rows"], 1);
    assert_eq!(filtered_payload["mode_supported"], 1);
    assert_eq!(filtered_payload["mode_unsupported"], 0);
    assert_eq!(filtered_payload["mode_supported_total"], 1);
    assert_eq!(filtered_payload["mode_unsupported_total"], 0);
    assert_eq!(filtered_payload["mode_counts_total"]["session_token"], 1);
    assert_eq!(filtered_payload["mode_counts"]["session_token"], 1);
    assert_eq!(filtered_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(filtered_payload["provider_counts"]["openai"], 1);
    assert_eq!(
        filtered_payload["source_kind_counts_total"]["credential_store"],
        1
    );
    assert_eq!(
        filtered_payload["source_kind_counts"]["credential_store"],
        1
    );
    assert_eq!(filtered_payload["revoked_counts_total"]["not_revoked"], 1);
    assert_eq!(filtered_payload["revoked_counts"]["not_revoked"], 1);
    assert_eq!(filtered_payload["entries"][0]["provider"], "openai");
    assert_eq!(filtered_payload["entries"][0]["mode_supported"], true);
    assert_eq!(filtered_payload["state_counts"]["ready"], 1);
    assert_eq!(filtered_payload["state_counts_total"]["ready"], 1);

    let zero_row_output = execute_auth_command(
        &config,
        "status openai --mode session-token --mode-support unsupported --source-kind credential-store --json",
    );
    let zero_row_payload: serde_json::Value =
        serde_json::from_str(&zero_row_output).expect("parse zero-row mode-support payload");
    assert_eq!(zero_row_payload["provider_filter"], "openai");
    assert_eq!(zero_row_payload["mode_filter"], "session_token");
    assert_eq!(zero_row_payload["mode_support_filter"], "unsupported");
    assert_eq!(zero_row_payload["source_kind_filter"], "credential_store");
    assert_eq!(zero_row_payload["revoked_filter"], "all");
    assert_eq!(zero_row_payload["rows_total"], 1);
    assert_eq!(zero_row_payload["rows"], 0);
    assert_eq!(zero_row_payload["mode_supported"], 0);
    assert_eq!(zero_row_payload["mode_unsupported"], 0);
    assert_eq!(zero_row_payload["mode_supported_total"], 1);
    assert_eq!(zero_row_payload["mode_unsupported_total"], 0);
    assert_eq!(zero_row_payload["mode_counts_total"]["session_token"], 1);
    assert_eq!(
        zero_row_payload["mode_counts"]
            .as_object()
            .expect("zero-row status mode-support mode counts")
            .len(),
        0
    );
    assert_eq!(zero_row_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(
        zero_row_payload["provider_counts"]
            .as_object()
            .expect("zero-row status mode-support provider counts")
            .len(),
        0
    );
    assert_eq!(
        zero_row_payload["source_kind_counts_total"]["credential_store"],
        1
    );
    assert_eq!(
        zero_row_payload["source_kind_counts"]
            .as_object()
            .expect("zero-row source-kind counts")
            .len(),
        0
    );
    assert_eq!(
        zero_row_payload["state_counts"]
            .as_object()
            .expect("zero-row state counts")
            .len(),
        0
    );
    assert_eq!(zero_row_payload["state_counts_total"]["ready"], 1);
    assert_eq!(zero_row_payload["revoked_counts_total"]["not_revoked"], 1);
    assert_eq!(
        zero_row_payload["revoked_counts"]
            .as_object()
            .expect("zero-row revoked status mode-support counts")
            .len(),
        0
    );

    let zero_row_text = execute_auth_command(
        &config,
        "status openai --mode session-token --mode-support unsupported --source-kind credential-store",
    );
    assert!(zero_row_text.contains("rows=0"));
    assert!(zero_row_text.contains("provider_filter=openai"));
    assert!(zero_row_text.contains("mode_filter=session_token"));
    assert!(zero_row_text.contains("source_kind_filter=credential_store"));
    assert!(zero_row_text.contains("revoked_filter=all"));
    assert!(zero_row_text.contains("mode_supported_total=1"));
    assert!(zero_row_text.contains("mode_unsupported_total=0"));
    assert!(zero_row_text.contains("mode_counts=none"));
    assert!(zero_row_text.contains("mode_counts_total=session_token:1"));
    assert!(zero_row_text.contains("provider_counts=none"));
    assert!(zero_row_text.contains("provider_counts_total=openai:1"));
    assert!(zero_row_text.contains("source_kind_counts=none"));
    assert!(zero_row_text.contains("source_kind_counts_total=credential_store:1"));
    assert!(zero_row_text.contains("revoked_counts=none"));
    assert!(zero_row_text.contains("revoked_counts_total=not_revoked:1"));
    assert!(zero_row_text.contains("mode_support_filter=unsupported"));
    assert!(zero_row_text.contains("state_counts=none"));
    assert!(zero_row_text.contains("state_counts_total=ready:1"));
}

#[test]
fn integration_execute_auth_command_status_revoked_filter_composes_with_other_filters() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("auth-status-revoked-composition.json");
    write_test_provider_credential(
        &store_path,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::SessionToken,
            access_token: Some("status-revoked-composition-access".to_string()),
            refresh_token: Some("status-revoked-composition-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(1200)),
            revoked: true,
        },
    );

    let mut config = test_auth_command_config();
    config.credential_store = store_path;
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.openai_auth_mode = ProviderAuthMethod::SessionToken;

    let revoked_output = execute_auth_command(
        &config,
        "status openai --mode session-token --mode-support supported --availability unavailable --state revoked --source-kind credential-store --revoked revoked --json",
    );
    let revoked_payload: serde_json::Value =
        serde_json::from_str(&revoked_output).expect("parse revoked composed status payload");
    assert_eq!(revoked_payload["provider_filter"], "openai");
    assert_eq!(revoked_payload["mode_filter"], "session_token");
    assert_eq!(revoked_payload["mode_support_filter"], "supported");
    assert_eq!(revoked_payload["availability_filter"], "unavailable");
    assert_eq!(revoked_payload["state_filter"], "revoked");
    assert_eq!(revoked_payload["source_kind_filter"], "credential_store");
    assert_eq!(revoked_payload["revoked_filter"], "revoked");
    assert_eq!(revoked_payload["rows_total"], 1);
    assert_eq!(revoked_payload["rows"], 1);
    assert_eq!(revoked_payload["mode_counts_total"]["session_token"], 1);
    assert_eq!(revoked_payload["mode_counts"]["session_token"], 1);
    assert_eq!(revoked_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(revoked_payload["provider_counts"]["openai"], 1);
    assert_eq!(revoked_payload["revoked_counts_total"]["revoked"], 1);
    assert_eq!(revoked_payload["revoked_counts"]["revoked"], 1);
    assert_eq!(revoked_payload["entries"][0]["revoked"], true);

    let zero_row_output = execute_auth_command(
        &config,
        "status openai --mode session-token --mode-support supported --availability unavailable --state revoked --source-kind credential-store --revoked not-revoked --json",
    );
    let zero_row_payload: serde_json::Value =
        serde_json::from_str(&zero_row_output).expect("parse zero-row revoked status payload");
    assert_eq!(zero_row_payload["revoked_filter"], "not_revoked");
    assert_eq!(zero_row_payload["rows_total"], 1);
    assert_eq!(zero_row_payload["rows"], 0);
    assert_eq!(zero_row_payload["mode_counts_total"]["session_token"], 1);
    assert_eq!(
        zero_row_payload["mode_counts"]
            .as_object()
            .expect("zero-row revoked status mode counts")
            .len(),
        0
    );
    assert_eq!(zero_row_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(
        zero_row_payload["provider_counts"]
            .as_object()
            .expect("zero-row revoked status provider counts")
            .len(),
        0
    );
    assert_eq!(zero_row_payload["revoked_counts_total"]["revoked"], 1);
    assert_eq!(
        zero_row_payload["revoked_counts"]
            .as_object()
            .expect("zero-row revoked status counts")
            .len(),
        0
    );
    assert_eq!(
        zero_row_payload["entries"]
            .as_array()
            .expect("zero-row revoked status entries")
            .len(),
        0
    );
}

#[test]
fn regression_execute_auth_command_availability_counts_zero_row_outputs_remain_explicit() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp
        .path()
        .join("auth-availability-zero-row-regression.json");
    write_test_provider_credential(
        &store_path,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::SessionToken,
            access_token: Some("availability-zero-row-access".to_string()),
            refresh_token: Some("availability-zero-row-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(1200)),
            revoked: false,
        },
    );

    let mut config = test_auth_command_config();
    config.credential_store = store_path;
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.openai_auth_mode = ProviderAuthMethod::SessionToken;

    let status_json = execute_auth_command(
        &config,
        "status openai --availability unavailable --state ready --source-kind credential-store --json",
    );
    let status_payload: serde_json::Value =
        serde_json::from_str(&status_json).expect("parse zero-row availability status payload");
    assert_eq!(status_payload["rows"], 0);
    assert_eq!(status_payload["availability_counts_total"]["available"], 1);
    assert_eq!(
        status_payload["availability_counts"]
            .as_object()
            .expect("zero-row status availability counts")
            .len(),
        0
    );
    let status_text = execute_auth_command(
        &config,
        "status openai --availability unavailable --state ready --source-kind credential-store",
    );
    assert!(status_text.contains("availability_counts=none"));
    assert!(status_text.contains("availability_counts_total=available:1"));

    let matrix_json = execute_auth_command(
        &config,
        "matrix openai --mode session-token --availability unavailable --state ready --source-kind credential-store --json",
    );
    let matrix_payload: serde_json::Value =
        serde_json::from_str(&matrix_json).expect("parse zero-row availability matrix payload");
    assert_eq!(matrix_payload["rows"], 0);
    assert_eq!(matrix_payload["availability_counts_total"]["available"], 1);
    assert_eq!(
        matrix_payload["availability_counts"]
            .as_object()
            .expect("zero-row matrix availability counts")
            .len(),
        0
    );
    let matrix_text = execute_auth_command(
        &config,
        "matrix openai --mode session-token --availability unavailable --state ready --source-kind credential-store",
    );
    assert!(matrix_text.contains("availability_counts=none"));
    assert!(matrix_text.contains("availability_counts_total=available:1"));
}

#[test]
fn regression_execute_auth_command_status_rejects_missing_and_duplicate_filter_flags() {
    let config = test_auth_command_config();

    let missing_mode = execute_auth_command(&config, "status --mode");
    assert!(missing_mode.contains("auth error: missing auth mode after --mode"));
    assert!(missing_mode.contains("usage: /auth status"));

    let missing_mode_support = execute_auth_command(&config, "status --mode-support");
    assert!(missing_mode_support
        .contains("auth error: missing mode-support filter after --mode-support"));
    assert!(missing_mode_support.contains("usage: /auth status"));

    let missing_availability = execute_auth_command(&config, "status --availability");
    assert!(missing_availability
        .contains("auth error: missing availability filter after --availability"));
    assert!(missing_availability.contains("usage: /auth status"));

    let duplicate_mode = execute_auth_command(&config, "status --mode api-key --mode adc");
    assert!(duplicate_mode.contains("auth error: duplicate --mode flag"));
    assert!(duplicate_mode.contains("usage: /auth status"));

    let duplicate_mode_support = execute_auth_command(
        &config,
        "status --mode-support all --mode-support supported",
    );
    assert!(duplicate_mode_support.contains("auth error: duplicate --mode-support flag"));
    assert!(duplicate_mode_support.contains("usage: /auth status"));

    let duplicate_state = execute_auth_command(&config, "status --state ready --state revoked");
    assert!(duplicate_state.contains("auth error: duplicate --state flag"));
    assert!(duplicate_state.contains("usage: /auth status"));

    let missing_source_kind = execute_auth_command(&config, "status --source-kind");
    assert!(
        missing_source_kind.contains("auth error: missing source-kind filter after --source-kind")
    );
    assert!(missing_source_kind.contains("usage: /auth status"));

    let duplicate_source_kind =
        execute_auth_command(&config, "status --source-kind all --source-kind env");
    assert!(duplicate_source_kind.contains("auth error: duplicate --source-kind flag"));
    assert!(duplicate_source_kind.contains("usage: /auth status"));

    let missing_revoked = execute_auth_command(&config, "status --revoked");
    assert!(missing_revoked.contains("auth error: missing revoked filter after --revoked"));
    assert!(missing_revoked.contains("usage: /auth status"));

    let duplicate_revoked = execute_auth_command(&config, "status --revoked all --revoked revoked");
    assert!(duplicate_revoked.contains("auth error: duplicate --revoked flag"));
    assert!(duplicate_revoked.contains("usage: /auth status"));
}

#[test]
fn functional_execute_integration_auth_command_set_status_rotate_revoke_lifecycle() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("integration-credentials.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;

    let set_output =
        execute_integration_auth_command(&config, "set github-token ghp_secret --json");
    let set_json: serde_json::Value = serde_json::from_str(&set_output).expect("parse set");
    assert_eq!(set_json["command"], "integration_auth.set");
    assert_eq!(set_json["integration_id"], "github-token");
    assert_eq!(set_json["status"], "saved");
    assert!(!set_output.contains("ghp_secret"));

    let status_output = execute_integration_auth_command(&config, "status github-token --json");
    let status_json: serde_json::Value =
        serde_json::from_str(&status_output).expect("parse status");
    assert_eq!(status_json["integrations_total"], 1);
    assert_eq!(status_json["integrations"], 1);
    assert_eq!(status_json["available_total"], 1);
    assert_eq!(status_json["unavailable_total"], 0);
    assert_eq!(status_json["available"], 1);
    assert_eq!(status_json["unavailable"], 0);
    assert_eq!(status_json["state_counts_total"]["ready"], 1);
    assert_eq!(status_json["state_counts"]["ready"], 1);
    assert_eq!(status_json["revoked_counts_total"]["not_revoked"], 1);
    assert_eq!(status_json["revoked_counts"]["not_revoked"], 1);
    assert_eq!(status_json["entries"][0]["integration_id"], "github-token");
    assert_eq!(status_json["entries"][0]["state"], "ready");
    assert_eq!(status_json["entries"][0]["revoked"], false);
    assert!(!status_output.contains("ghp_secret"));

    let rotate_output =
        execute_integration_auth_command(&config, "rotate github-token ghp_rotated --json");
    let rotate_json: serde_json::Value =
        serde_json::from_str(&rotate_output).expect("parse rotate");
    assert_eq!(rotate_json["command"], "integration_auth.rotate");
    assert_eq!(rotate_json["status"], "rotated");
    assert!(!rotate_output.contains("ghp_rotated"));

    let revoke_output = execute_integration_auth_command(&config, "revoke github-token --json");
    let revoke_json: serde_json::Value =
        serde_json::from_str(&revoke_output).expect("parse revoke");
    assert_eq!(revoke_json["command"], "integration_auth.revoke");
    assert_eq!(revoke_json["status"], "revoked");

    let post_revoke_status =
        execute_integration_auth_command(&config, "status github-token --json");
    let post_revoke_json: serde_json::Value =
        serde_json::from_str(&post_revoke_status).expect("parse status");
    assert_eq!(post_revoke_json["available_total"], 0);
    assert_eq!(post_revoke_json["unavailable_total"], 1);
    assert_eq!(post_revoke_json["available"], 0);
    assert_eq!(post_revoke_json["unavailable"], 1);
    assert_eq!(post_revoke_json["state_counts_total"]["revoked"], 1);
    assert_eq!(post_revoke_json["state_counts"]["revoked"], 1);
    assert_eq!(post_revoke_json["revoked_counts_total"]["revoked"], 1);
    assert_eq!(post_revoke_json["revoked_counts"]["revoked"], 1);
    assert_eq!(post_revoke_json["entries"][0]["state"], "revoked");
    assert_eq!(post_revoke_json["entries"][0]["available"], false);

    let store = load_credential_store(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
    )
    .expect("load credential store");
    let entry = store
        .integrations
        .get("github-token")
        .expect("github integration entry");
    assert!(entry.secret.is_none());
    assert!(entry.revoked);
}

#[test]
fn functional_execute_integration_auth_command_status_reports_totals_with_filter() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("integration-status-totals.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;

    execute_integration_auth_command(&config, "set github-token ghp_ready --json");
    execute_integration_auth_command(&config, "set slack-token slack_ready --json");
    execute_integration_auth_command(&config, "revoke slack-token --json");

    let status_output = execute_integration_auth_command(&config, "status github-token --json");
    let status_json: serde_json::Value =
        serde_json::from_str(&status_output).expect("parse status totals");
    assert_eq!(status_json["integrations_total"], 2);
    assert_eq!(status_json["integrations"], 1);
    assert_eq!(status_json["available_total"], 1);
    assert_eq!(status_json["unavailable_total"], 1);
    assert_eq!(status_json["available"], 1);
    assert_eq!(status_json["unavailable"], 0);
    assert_eq!(status_json["state_counts_total"]["ready"], 1);
    assert_eq!(status_json["state_counts_total"]["revoked"], 1);
    assert_eq!(status_json["state_counts"]["ready"], 1);
    assert_eq!(status_json["revoked_counts_total"]["not_revoked"], 1);
    assert_eq!(status_json["revoked_counts_total"]["revoked"], 1);
    assert_eq!(status_json["revoked_counts"]["not_revoked"], 1);

    let text_output = execute_integration_auth_command(&config, "status github-token");
    assert!(text_output.contains("integrations=1"));
    assert!(text_output.contains("integrations_total=2"));
    assert!(text_output.contains("available=1"));
    assert!(text_output.contains("unavailable=0"));
    assert!(text_output.contains("available_total=1"));
    assert!(text_output.contains("unavailable_total=1"));
    assert!(text_output.contains("state_counts=ready:1"));
    assert!(text_output.contains("state_counts_total=ready:1,revoked:1"));
    assert!(text_output.contains("revoked_counts=not_revoked:1"));
    assert!(text_output.contains("revoked_counts_total=not_revoked:1,revoked:1"));
}

#[test]
fn regression_execute_integration_auth_command_status_handles_empty_store() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("integration-status-empty.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;

    let store = CredentialStoreData {
        encryption: CredentialStoreEncryptionMode::None,
        providers: BTreeMap::new(),
        integrations: BTreeMap::new(),
    };
    save_credential_store(&config.credential_store, &store, None)
        .expect("save empty credential store");

    let output = execute_integration_auth_command(&config, "status --json");
    let payload: serde_json::Value =
        serde_json::from_str(&output).expect("parse empty integration status");
    assert_eq!(payload["integrations_total"], 0);
    assert_eq!(payload["integrations"], 0);
    assert_eq!(payload["available_total"], 0);
    assert_eq!(payload["unavailable_total"], 0);
    assert_eq!(payload["available"], 0);
    assert_eq!(payload["unavailable"], 0);
    assert_eq!(
        payload["state_counts_total"]
            .as_object()
            .expect("empty state counts total")
            .len(),
        0
    );
    assert_eq!(
        payload["state_counts"]
            .as_object()
            .expect("empty state counts")
            .len(),
        0
    );
    assert_eq!(
        payload["revoked_counts_total"]
            .as_object()
            .expect("empty revoked counts total")
            .len(),
        0
    );
    assert_eq!(
        payload["revoked_counts"]
            .as_object()
            .expect("empty revoked counts")
            .len(),
        0
    );

    let text_output = execute_integration_auth_command(&config, "status");
    assert!(text_output.contains("integrations=0"));
    assert!(text_output.contains("integrations_total=0"));
    assert!(text_output.contains("available=0"));
    assert!(text_output.contains("unavailable=0"));
    assert!(text_output.contains("available_total=0"));
    assert!(text_output.contains("unavailable_total=0"));
    assert!(text_output.contains("state_counts=none"));
    assert!(text_output.contains("state_counts_total=none"));
    assert!(text_output.contains("revoked_counts=none"));
    assert!(text_output.contains("revoked_counts_total=none"));
}

#[test]
fn regression_execute_auth_command_login_rejects_unsupported_google_session_mode() {
    let config = test_auth_command_config();
    let output = execute_auth_command(&config, "login google --mode session-token --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse output");
    assert_eq!(payload["status"], "error");
    assert!(payload["reason"]
        .as_str()
        .unwrap_or_default()
        .contains("not supported"));
    assert_eq!(
        payload["supported_modes"],
        serde_json::json!(["api_key", "oauth_token", "adc"])
    );
}

#[cfg(unix)]
#[test]
fn functional_execute_auth_command_login_openai_launch_executes_codex_login_command() {
    let temp = tempdir().expect("tempdir");
    let args_file = temp.path().join("codex-login-args.txt");
    let script = write_mock_codex_script(
        temp.path(),
        &format!("printf '%s' \"$*\" > \"{}\"", args_file.display()),
    );

    let mut config = test_auth_command_config();
    config.openai_codex_backend = true;
    config.openai_codex_cli = script.display().to_string();

    let output = execute_auth_command(&config, "login openai --mode oauth-token --launch --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse output");
    assert_eq!(payload["status"], "launched");
    assert_eq!(payload["source"], "codex_cli");
    assert_eq!(payload["launch_requested"], true);
    assert_eq!(payload["launch_executed"], true);
    assert_eq!(
        payload["launch_command"],
        format!("{} --login", script.display())
    );

    let launched_args = std::fs::read_to_string(&args_file).expect("read codex login args");
    assert_eq!(launched_args, "--login");
}

#[cfg(unix)]
#[test]
fn integration_execute_auth_command_login_google_adc_launch_executes_gcloud_flow() {
    let temp = tempdir().expect("tempdir");
    let gcloud_args = temp.path().join("gcloud-login-args.txt");
    let gemini = write_mock_gemini_script(temp.path(), "printf 'ok'");
    let gcloud = write_mock_gcloud_script(
        temp.path(),
        &format!("printf '%s' \"$*\" > \"{}\"", gcloud_args.display()),
    );

    let mut config = test_auth_command_config();
    config.google_gemini_backend = true;
    config.google_gemini_cli = gemini.display().to_string();
    config.google_gcloud_cli = gcloud.display().to_string();

    let output = execute_auth_command(&config, "login google --mode adc --launch --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse output");
    assert_eq!(payload["status"], "launched");
    assert_eq!(payload["source"], "gemini_cli");
    assert_eq!(payload["launch_requested"], true);
    assert_eq!(payload["launch_executed"], true);
    assert_eq!(
        payload["launch_command"],
        format!("{} auth application-default login", gcloud.display())
    );

    let launched_args = std::fs::read_to_string(&gcloud_args).expect("read gcloud args");
    assert_eq!(launched_args, "auth application-default login");
}

#[test]
fn regression_execute_auth_command_login_launch_rejects_unsupported_api_key_mode() {
    let config = test_auth_command_config();
    let output = execute_auth_command(&config, "login openai --mode api-key --launch --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse output");
    assert_eq!(payload["status"], "error");
    assert_eq!(payload["launch_requested"], true);
    assert_eq!(payload["launch_executed"], false);
    assert!(payload["reason"]
        .as_str()
        .unwrap_or_default()
        .contains("--launch is only supported"));
}

#[cfg(unix)]
#[test]
fn regression_execute_auth_command_login_launch_reports_non_zero_exit() {
    let temp = tempdir().expect("tempdir");
    let script = write_mock_claude_script(temp.path(), "exit 9");

    let mut config = test_auth_command_config();
    config.anthropic_claude_backend = true;
    config.anthropic_claude_cli = script.display().to_string();

    let output = execute_auth_command(
        &config,
        "login anthropic --mode oauth-token --launch --json",
    );
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse output");
    assert_eq!(payload["status"], "error");
    assert_eq!(payload["launch_requested"], true);
    assert_eq!(payload["launch_executed"], false);
    assert_eq!(payload["launch_command"], script.display().to_string());
    assert!(payload["reason"]
        .as_str()
        .unwrap_or_default()
        .contains("exited with status 9"));
}

#[test]
fn functional_execute_auth_command_login_anthropic_oauth_reports_backend_ready() {
    let mut config = test_auth_command_config();
    config.anthropic_claude_backend = true;
    config.anthropic_claude_cli = std::env::current_exe()
        .expect("current executable path")
        .display()
        .to_string();

    let output = execute_auth_command(&config, "login anthropic --mode oauth-token --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse output");
    assert_eq!(payload["status"], "ready");
    assert_eq!(payload["source"], "claude_cli");
    assert_eq!(payload["persisted"], false);
    assert_eq!(payload["launch_requested"], false);
    assert_eq!(payload["launch_executed"], false);
    assert!(payload["action"]
        .as_str()
        .unwrap_or_default()
        .contains("enter /login in the Claude prompt"));
}

#[test]
fn regression_execute_auth_command_status_anthropic_oauth_reports_backend_disabled() {
    let mut config = test_auth_command_config();
    config.anthropic_auth_mode = ProviderAuthMethod::OauthToken;
    config.anthropic_claude_backend = false;

    let output = execute_auth_command(&config, "status anthropic --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse status payload");
    let entry = payload["entries"]
        .as_array()
        .and_then(|entries| entries.first())
        .expect("anthropic status entry");
    assert_eq!(entry["provider"], "anthropic");
    assert_eq!(entry["mode"], "oauth_token");
    assert_eq!(entry["mode_supported"], true);
    assert_eq!(entry["available"], false);
    assert_eq!(entry["state"], "backend_disabled");
}

#[test]
fn regression_execute_auth_command_status_anthropic_oauth_reports_backend_unavailable() {
    let mut config = test_auth_command_config();
    config.anthropic_auth_mode = ProviderAuthMethod::OauthToken;
    config.anthropic_claude_backend = true;
    config.anthropic_claude_cli = "__missing_claude_backend_for_test__".to_string();

    let output = execute_auth_command(&config, "status anthropic --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse status payload");
    let entry = payload["entries"]
        .as_array()
        .and_then(|entries| entries.first())
        .expect("anthropic status entry");
    assert_eq!(entry["provider"], "anthropic");
    assert_eq!(entry["mode"], "oauth_token");
    assert_eq!(entry["mode_supported"], true);
    assert_eq!(entry["available"], false);
    assert_eq!(entry["state"], "backend_unavailable");
}

#[test]
fn functional_execute_auth_command_login_google_oauth_reports_backend_ready() {
    let mut config = test_auth_command_config();
    config.google_gemini_backend = true;
    config.google_gemini_cli = std::env::current_exe()
        .expect("current executable path")
        .display()
        .to_string();

    let output = execute_auth_command(&config, "login google --mode oauth-token --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse output");
    assert_eq!(payload["status"], "ready");
    assert_eq!(payload["source"], "gemini_cli");
    assert_eq!(payload["persisted"], false);
    assert_eq!(payload["launch_requested"], false);
    assert_eq!(payload["launch_executed"], false);
    assert!(payload["action"]
        .as_str()
        .unwrap_or_default()
        .contains("Login with Google"));
}

#[test]
fn regression_execute_auth_command_status_google_oauth_reports_backend_disabled() {
    let mut config = test_auth_command_config();
    config.google_auth_mode = ProviderAuthMethod::OauthToken;
    config.google_gemini_backend = false;

    let output = execute_auth_command(&config, "status google --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse status payload");
    let entry = payload["entries"]
        .as_array()
        .and_then(|entries| entries.first())
        .expect("google status entry");
    assert_eq!(entry["provider"], "google");
    assert_eq!(entry["mode"], "oauth_token");
    assert_eq!(entry["mode_supported"], true);
    assert_eq!(entry["available"], false);
    assert_eq!(entry["state"], "backend_disabled");
}

#[test]
fn regression_execute_auth_command_status_google_oauth_reports_backend_unavailable() {
    let mut config = test_auth_command_config();
    config.google_auth_mode = ProviderAuthMethod::OauthToken;
    config.google_gemini_backend = true;
    config.google_gemini_cli = "__missing_gemini_backend_for_test__".to_string();

    let output = execute_auth_command(&config, "status google --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse status payload");
    let entry = payload["entries"]
        .as_array()
        .and_then(|entries| entries.first())
        .expect("google status entry");
    assert_eq!(entry["provider"], "google");
    assert_eq!(entry["mode"], "oauth_token");
    assert_eq!(entry["mode_supported"], true);
    assert_eq!(entry["available"], false);
    assert_eq!(entry["state"], "backend_unavailable");
}

#[test]
fn unit_cli_skills_lock_flags_default_to_disabled() {
    let cli = Cli::parse_from(["tau-rs"]);
    assert!(!cli.skills_lock_write);
    assert!(!cli.skills_sync);
    assert!(cli.skills_lock_file.is_none());
}

#[test]
fn functional_cli_skills_lock_flags_accept_explicit_values() {
    let cli = Cli::parse_from([
        "tau-rs",
        "--skills-lock-write",
        "--skills-sync",
        "--skills-lock-file",
        "custom/skills.lock.json",
    ]);
    assert!(cli.skills_lock_write);
    assert!(cli.skills_sync);
    assert_eq!(
        cli.skills_lock_file,
        Some(PathBuf::from("custom/skills.lock.json"))
    );
}

#[test]
fn unit_cli_skills_cache_flags_default_to_online_mode() {
    let cli = Cli::parse_from(["tau-rs"]);
    assert!(!cli.skills_offline);
    assert!(cli.skills_cache_dir.is_none());
}

#[test]
fn functional_cli_skills_cache_flags_accept_explicit_values() {
    let cli = Cli::parse_from([
        "tau-rs",
        "--skills-offline",
        "--skills-cache-dir",
        "custom/skills-cache",
    ]);
    assert!(cli.skills_offline);
    assert_eq!(
        cli.skills_cache_dir,
        Some(PathBuf::from("custom/skills-cache"))
    );
}

#[test]
fn unit_cli_command_file_flags_default_to_disabled() {
    let cli = Cli::parse_from(["tau-rs"]);
    assert!(cli.command_file.is_none());
    assert_eq!(
        cli.command_file_error_mode,
        CliCommandFileErrorMode::FailFast
    );
}

#[test]
fn functional_cli_command_file_flags_accept_overrides() {
    let cli = Cli::parse_from([
        "tau-rs",
        "--command-file",
        "automation.commands",
        "--command-file-error-mode",
        "continue-on-error",
    ]);
    assert_eq!(cli.command_file, Some(PathBuf::from("automation.commands")));
    assert_eq!(
        cli.command_file_error_mode,
        CliCommandFileErrorMode::ContinueOnError
    );
}

#[test]
fn unit_cli_onboarding_flags_default_to_disabled() {
    let cli = Cli::parse_from(["tau-rs"]);
    assert!(!cli.onboard);
    assert!(!cli.onboard_non_interactive);
    assert_eq!(cli.onboard_profile, "default");
}

#[test]
fn functional_cli_onboarding_flags_accept_explicit_overrides() {
    let cli = Cli::parse_from([
        "tau-rs",
        "--onboard",
        "--onboard-non-interactive",
        "--onboard-profile",
        "team_default",
    ]);
    assert!(cli.onboard);
    assert!(cli.onboard_non_interactive);
    assert_eq!(cli.onboard_profile, "team_default");
}

#[test]
fn regression_cli_onboarding_non_interactive_requires_onboard() {
    let parse = Cli::try_parse_from(["tau-rs", "--onboard-non-interactive"]);
    let error = parse.expect_err("non-interactive onboarding should require --onboard");
    assert!(error.to_string().contains("--onboard"));
}

#[test]
fn regression_cli_onboarding_profile_requires_onboard() {
    let parse = Cli::try_parse_from(["tau-rs", "--onboard-profile", "team"]);
    let error = parse.expect_err("onboarding profile should require --onboard");
    assert!(error.to_string().contains("--onboard"));
}

#[test]
fn unit_is_retryable_provider_error_classifies_status_errors() {
    assert!(is_retryable_provider_error(&TauAiError::HttpStatus {
        status: 429,
        body: "rate limited".to_string(),
    }));
    assert!(is_retryable_provider_error(&TauAiError::HttpStatus {
        status: 503,
        body: "unavailable".to_string(),
    }));
    assert!(!is_retryable_provider_error(&TauAiError::HttpStatus {
        status: 401,
        body: "unauthorized".to_string(),
    }));
    assert!(!is_retryable_provider_error(&TauAiError::InvalidResponse(
        "bad payload".to_string(),
    )));
}

#[test]
fn functional_resolve_fallback_models_parses_deduplicates_and_skips_primary() {
    let primary = ModelRef::parse("openai/gpt-4o-mini").expect("primary model parse");
    let mut cli = test_cli();
    cli.fallback_model = vec![
        "openai/gpt-4o-mini".to_string(),
        "google/gemini-2.5-pro".to_string(),
        "google/gemini-2.5-pro".to_string(),
        "anthropic/claude-sonnet-4-20250514".to_string(),
    ];

    let resolved = resolve_fallback_models(&cli, &primary).expect("resolve fallbacks");
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0].provider, Provider::Google);
    assert_eq!(resolved[0].model, "gemini-2.5-pro");
    assert_eq!(resolved[1].provider, Provider::Anthropic);
}

#[tokio::test]
async fn functional_fallback_routing_client_uses_next_route_for_retryable_error() {
    let primary = Arc::new(SequenceClient {
        outcomes: AsyncMutex::new(VecDeque::from([Err(TauAiError::HttpStatus {
            status: 503,
            body: "unavailable".to_string(),
        })])),
    });
    let fallback = Arc::new(SequenceClient {
        outcomes: AsyncMutex::new(VecDeque::from([Ok(ChatResponse {
            message: Message::assistant_text("fallback success"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        })])),
    });

    let client = FallbackRoutingClient::new(
        vec![
            ClientRoute {
                provider: Provider::OpenAi,
                model: "gpt-primary".to_string(),
                client: primary as Arc<dyn LlmClient>,
            },
            ClientRoute {
                provider: Provider::Anthropic,
                model: "claude-fallback".to_string(),
                client: fallback as Arc<dyn LlmClient>,
            },
        ],
        None,
    );

    let response = client
        .complete(test_chat_request())
        .await
        .expect("fallback should recover request");
    assert_eq!(response.message.text_content(), "fallback success");
}

#[tokio::test]
async fn regression_fallback_routing_client_skips_fallback_on_non_retryable_error() {
    let primary = Arc::new(SequenceClient {
        outcomes: AsyncMutex::new(VecDeque::from([Err(TauAiError::HttpStatus {
            status: 400,
            body: "bad request".to_string(),
        })])),
    });
    let fallback = Arc::new(SequenceClient {
        outcomes: AsyncMutex::new(VecDeque::from([Ok(ChatResponse {
            message: Message::assistant_text("should not run"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        })])),
    });

    let client = FallbackRoutingClient::new(
        vec![
            ClientRoute {
                provider: Provider::OpenAi,
                model: "gpt-primary".to_string(),
                client: primary as Arc<dyn LlmClient>,
            },
            ClientRoute {
                provider: Provider::Google,
                model: "gemini-fallback".to_string(),
                client: fallback.clone() as Arc<dyn LlmClient>,
            },
        ],
        None,
    );

    let error = client
        .complete(test_chat_request())
        .await
        .expect_err("non-retryable error should return immediately");
    match error {
        TauAiError::HttpStatus { status, body } => {
            assert_eq!(status, 400);
            assert!(body.contains("bad request"));
        }
        other => panic!("expected HttpStatus error, got {other:?}"),
    }

    let fallback_remaining = fallback.outcomes.lock().await.len();
    assert_eq!(
        fallback_remaining, 1,
        "fallback route should not be invoked"
    );
}

#[tokio::test]
async fn integration_fallback_routing_client_emits_json_event_on_failover() {
    let primary = Arc::new(SequenceClient {
        outcomes: AsyncMutex::new(VecDeque::from([Err(TauAiError::HttpStatus {
            status: 429,
            body: "rate limited".to_string(),
        })])),
    });
    let fallback = Arc::new(SequenceClient {
        outcomes: AsyncMutex::new(VecDeque::from([Ok(ChatResponse {
            message: Message::assistant_text("fallback ok"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        })])),
    });
    let events = Arc::new(std::sync::Mutex::new(Vec::<serde_json::Value>::new()));
    let sink_events = events.clone();
    let sink = Arc::new(move |event: serde_json::Value| {
        sink_events.lock().expect("event lock").push(event);
    });

    let client = FallbackRoutingClient::new(
        vec![
            ClientRoute {
                provider: Provider::OpenAi,
                model: "gpt-primary".to_string(),
                client: primary as Arc<dyn LlmClient>,
            },
            ClientRoute {
                provider: Provider::OpenAi,
                model: "gpt-fallback".to_string(),
                client: fallback as Arc<dyn LlmClient>,
            },
        ],
        Some(sink),
    );

    let _ = client
        .complete(test_chat_request())
        .await
        .expect("fallback should succeed");

    let events = events.lock().expect("event lock");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["type"], "provider_fallback");
    assert_eq!(events[0]["from_model"], "openai/gpt-primary");
    assert_eq!(events[0]["to_model"], "openai/gpt-fallback");
    assert_eq!(events[0]["error_kind"], "http_status");
    assert_eq!(events[0]["status"], 429);
    assert_eq!(events[0]["fallback_index"], 1);
}

#[test]
fn resolve_api_key_returns_none_when_all_candidates_are_empty() {
    let key = resolve_api_key(vec![None, Some("".to_string())]);
    assert!(key.is_none());
}

#[test]
fn functional_openai_api_key_candidates_include_openrouter_groq_xai_mistral_and_azure_env_slots() {
    let candidates =
        provider_api_key_candidates_with_inputs(Provider::OpenAi, None, None, None, None);
    assert!(candidates
        .iter()
        .any(|(source, _)| *source == "OPENROUTER_API_KEY"));
    assert!(candidates
        .iter()
        .any(|(source, _)| *source == "GROQ_API_KEY"));
    assert!(candidates
        .iter()
        .any(|(source, _)| *source == "XAI_API_KEY"));
    assert!(candidates
        .iter()
        .any(|(source, _)| *source == "MISTRAL_API_KEY"));
    assert!(candidates
        .iter()
        .any(|(source, _)| *source == "AZURE_OPENAI_API_KEY"));
}

#[test]
fn unit_provider_auth_capability_reports_api_key_support() {
    let openai = provider_auth_capability(Provider::OpenAi, ProviderAuthMethod::ApiKey);
    assert!(openai.supported);
    assert_eq!(openai.reason, "supported");

    let anthropic = provider_auth_capability(Provider::Anthropic, ProviderAuthMethod::OauthToken);
    assert!(anthropic.supported);
    assert_eq!(anthropic.reason, "supported");

    let google = provider_auth_capability(Provider::Google, ProviderAuthMethod::OauthToken);
    assert!(google.supported);
    assert_eq!(google.reason, "supported");
}

#[test]
fn regression_build_provider_client_anthropic_oauth_mode_requires_backend_when_disabled() {
    let mut cli = test_cli();
    cli.anthropic_auth_mode = CliProviderAuthMode::OauthToken;
    cli.anthropic_claude_backend = false;

    match build_provider_client(&cli, Provider::Anthropic) {
        Ok(_) => panic!("oauth mode without backend should fail"),
        Err(error) => {
            assert!(error.to_string().contains("requires Claude Code backend"));
        }
    }
}

#[test]
fn regression_build_provider_client_google_oauth_mode_requires_backend_when_disabled() {
    let mut cli = test_cli();
    cli.google_auth_mode = CliProviderAuthMode::OauthToken;
    cli.google_gemini_backend = false;

    match build_provider_client(&cli, Provider::Google) {
        Ok(_) => panic!("oauth mode without backend should fail"),
        Err(error) => {
            assert!(error.to_string().contains("requires Gemini CLI backend"));
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn integration_build_provider_client_uses_claude_backend_for_anthropic_oauth_mode() {
    let temp = tempdir().expect("tempdir");
    let script = write_mock_claude_script(
        temp.path(),
        r#"
if [ "$1" != "-p" ]; then
  echo "missing -p" >&2
  exit 8
fi
printf '{"type":"result","subtype":"success","is_error":false,"result":"claude backend response"}'
"#,
    );

    let mut cli = test_cli();
    cli.anthropic_auth_mode = CliProviderAuthMode::OauthToken;
    cli.anthropic_claude_backend = true;
    cli.anthropic_claude_cli = script.display().to_string();
    cli.anthropic_claude_timeout_ms = 2_000;
    cli.anthropic_api_key = None;

    let client =
        build_provider_client(&cli, Provider::Anthropic).expect("build claude backend client");
    let response = client
        .complete(test_chat_request())
        .await
        .expect("claude backend completion");
    assert_eq!(response.message.text_content(), "claude backend response");
}

#[cfg(unix)]
#[tokio::test]
async fn integration_build_provider_client_uses_gemini_backend_for_google_oauth_mode() {
    let temp = tempdir().expect("tempdir");
    let script = write_mock_gemini_script(
        temp.path(),
        r#"
if [ "$1" != "-p" ]; then
  echo "missing -p" >&2
  exit 8
fi
printf '{"response":"gemini backend response"}'
"#,
    );

    let mut cli = test_cli();
    cli.google_auth_mode = CliProviderAuthMode::OauthToken;
    cli.google_gemini_backend = true;
    cli.google_gemini_cli = script.display().to_string();
    cli.google_gemini_timeout_ms = 2_000;
    cli.google_api_key = None;

    let client =
        build_provider_client(&cli, Provider::Google).expect("build gemini backend client");
    let response = client
        .complete(test_chat_request())
        .await
        .expect("gemini backend completion");
    assert_eq!(response.message.text_content(), "gemini backend response");
}

#[test]
fn integration_build_provider_client_preserves_api_key_mode_behavior() {
    let mut cli = test_cli();
    cli.openai_api_key = Some("test-openai-key".to_string());

    let client = build_provider_client(&cli, Provider::OpenAi).expect("build client");
    let ptr = Arc::as_ptr(&client);
    assert!(!ptr.is_null());
}

#[test]
fn unit_encrypt_and_decrypt_credential_store_secret_roundtrip_keyed() {
    let secret = "secret-token-123";
    let encoded = encrypt_credential_store_secret(
        secret,
        CredentialStoreEncryptionMode::Keyed,
        Some("very-strong-key"),
    )
    .expect("encode credential");
    assert!(encoded.starts_with("enc:v1:"));
    assert!(!encoded.contains(secret));

    let decoded = decrypt_credential_store_secret(
        &encoded,
        CredentialStoreEncryptionMode::Keyed,
        Some("very-strong-key"),
    )
    .expect("decode credential");
    assert_eq!(decoded, secret);
}

#[test]
fn regression_decrypt_credential_store_secret_rejects_wrong_key() {
    let encoded = encrypt_credential_store_secret(
        "secret-token-xyz",
        CredentialStoreEncryptionMode::Keyed,
        Some("correct-key-123"),
    )
    .expect("encode credential");

    let error = decrypt_credential_store_secret(
        &encoded,
        CredentialStoreEncryptionMode::Keyed,
        Some("wrong-key-123"),
    )
    .expect_err("wrong key should fail");
    assert!(error.to_string().contains("integrity check failed"));
}

#[test]
fn functional_credential_store_roundtrip_preserves_provider_records() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("credentials.json");
    write_test_provider_credential(
        &store_path,
        CredentialStoreEncryptionMode::Keyed,
        Some("credential-key"),
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("openai-access".to_string()),
            refresh_token: Some("openai-refresh".to_string()),
            expires_unix: Some(12345),
            revoked: false,
        },
    );

    let loaded = load_credential_store(
        &store_path,
        CredentialStoreEncryptionMode::None,
        Some("credential-key"),
    )
    .expect("load credential store");
    let entry = loaded
        .providers
        .get("openai")
        .expect("openai entry should exist");
    assert_eq!(entry.auth_method, ProviderAuthMethod::OauthToken);
    assert_eq!(entry.access_token.as_deref(), Some("openai-access"));
    assert_eq!(entry.refresh_token.as_deref(), Some("openai-refresh"));
    assert_eq!(entry.expires_unix, Some(12345));
    assert!(!entry.revoked);
}

#[test]
fn integration_credential_store_roundtrip_preserves_integration_records() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("integration-credentials.json");
    write_test_integration_credential(
        &store_path,
        CredentialStoreEncryptionMode::Keyed,
        Some("credential-key"),
        "github-token",
        IntegrationCredentialStoreRecord {
            secret: Some("ghp_top_secret".to_string()),
            revoked: false,
            updated_unix: Some(98765),
        },
    );

    let loaded = load_credential_store(
        &store_path,
        CredentialStoreEncryptionMode::None,
        Some("credential-key"),
    )
    .expect("load credential store");
    let entry = loaded
        .integrations
        .get("github-token")
        .expect("integration entry");
    assert_eq!(entry.secret.as_deref(), Some("ghp_top_secret"));
    assert!(!entry.revoked);
    assert_eq!(entry.updated_unix, Some(98765));
}

#[test]
fn regression_load_credential_store_allows_legacy_provider_only_payload() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("legacy-credentials.json");
    std::fs::write(
        &store_path,
        r#"{
  "schema_version": 1,
  "encryption": "none",
  "providers": {
    "openai": {
      "auth_method": "oauth_token",
      "access_token": "legacy-access",
      "refresh_token": "legacy-refresh",
      "expires_unix": 42,
      "revoked": false
    }
  }
}
"#,
    )
    .expect("write legacy credential store");

    let loaded = load_credential_store(&store_path, CredentialStoreEncryptionMode::None, None)
        .expect("load legacy credential store");
    assert!(loaded.integrations.is_empty());
    assert_eq!(
        loaded
            .providers
            .get("openai")
            .and_then(|entry| entry.access_token.as_deref()),
        Some("legacy-access")
    );
}

#[test]
fn functional_resolve_store_backed_provider_credential_refreshes_expired_token() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("credentials.json");
    let now = current_unix_timestamp();

    write_test_provider_credential(
        &store_path,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("stale-access".to_string()),
            refresh_token: Some("refresh-token".to_string()),
            expires_unix: Some(now.saturating_sub(30)),
            revoked: false,
        },
    );

    let mut cli = test_cli();
    cli.credential_store = store_path.clone();
    cli.openai_auth_mode = CliProviderAuthMode::OauthToken;
    cli.credential_store_encryption = CliCredentialStoreEncryptionMode::None;

    let resolved = resolve_store_backed_provider_credential(
        &cli,
        Provider::OpenAi,
        ProviderAuthMethod::OauthToken,
    )
    .expect("resolve refreshed credential");
    assert_eq!(resolved.method, ProviderAuthMethod::OauthToken);
    assert_eq!(resolved.source.as_deref(), Some("credential_store"));
    let access = resolved.secret.expect("access token");
    assert!(access.starts_with("openai_access_"));
    assert_ne!(access, "stale-access");

    let persisted = load_credential_store(&store_path, CredentialStoreEncryptionMode::None, None)
        .expect("reload store");
    let entry = persisted.providers.get("openai").expect("openai entry");
    assert_eq!(entry.access_token.as_deref(), Some(access.as_str()));
    assert!(entry.expires_unix.unwrap_or(0) > now);
}

#[test]
fn functional_refresh_provider_access_token_generates_deterministic_shape() {
    let refreshed = refresh_provider_access_token(Provider::OpenAi, "refresh-token", 1700)
        .expect("refresh token");
    assert!(refreshed.access_token.starts_with("openai_access_"));
    assert!(refreshed
        .refresh_token
        .as_deref()
        .unwrap_or_default()
        .starts_with("openai_refresh_"));
    assert_eq!(refreshed.expires_unix, Some(1700 + 3600));
}

#[test]
fn regression_resolve_store_backed_provider_credential_marks_revoked_refresh_token() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("credentials.json");
    let now = current_unix_timestamp();

    write_test_provider_credential(
        &store_path,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("stale-access".to_string()),
            refresh_token: Some("revoked-refresh-token".to_string()),
            expires_unix: Some(now.saturating_sub(5)),
            revoked: false,
        },
    );

    let mut cli = test_cli();
    cli.credential_store = store_path.clone();
    cli.openai_auth_mode = CliProviderAuthMode::OauthToken;
    cli.credential_store_encryption = CliCredentialStoreEncryptionMode::None;

    let error = resolve_store_backed_provider_credential(
        &cli,
        Provider::OpenAi,
        ProviderAuthMethod::OauthToken,
    )
    .expect_err("revoked refresh should require re-auth");
    assert!(error.to_string().contains("requires re-authentication"));
    assert!(error.to_string().contains("revoked"));

    let persisted = load_credential_store(&store_path, CredentialStoreEncryptionMode::None, None)
        .expect("reload store");
    let entry = persisted.providers.get("openai").expect("openai entry");
    assert!(entry.revoked);
}

#[test]
fn regression_resolve_store_backed_provider_credential_hides_corrupted_payload_values() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("credentials.json");
    let leaked_value = "leaky-secret-token";
    let payload = format!(
            "{{\"schema_version\":1,\"encryption\":\"keyed\",\"providers\":{{\"openai\":{{\"auth_method\":\"oauth_token\",\"access_token\":\"enc:v1:not-base64-{leaked_value}\",\"refresh_token\":null,\"expires_unix\":null,\"revoked\":false}}}}}}"
        );
    std::fs::write(&store_path, payload).expect("write corrupted store");

    let mut cli = test_cli();
    cli.credential_store = store_path;
    cli.credential_store_key = Some("valid-key-123".to_string());
    cli.credential_store_encryption = CliCredentialStoreEncryptionMode::Keyed;
    cli.openai_auth_mode = CliProviderAuthMode::OauthToken;

    let error = resolve_store_backed_provider_credential(
        &cli,
        Provider::OpenAi,
        ProviderAuthMethod::OauthToken,
    )
    .expect_err("corrupted store should fail");
    let message = error.to_string();
    assert!(
        message.contains("failed to load provider credential store")
            || message.contains("invalid or corrupted")
    );
    assert!(!error.to_string().contains(leaked_value));
}

#[test]
fn integration_build_provider_client_supports_openai_oauth_from_credential_store() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("credentials.json");
    write_test_provider_credential(
        &store_path,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("openai-oauth-access".to_string()),
            refresh_token: Some("refresh-token".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(900)),
            revoked: false,
        },
    );

    let mut cli = test_cli();
    cli.openai_auth_mode = CliProviderAuthMode::OauthToken;
    cli.credential_store = store_path;
    cli.credential_store_encryption = CliCredentialStoreEncryptionMode::None;

    let client = build_provider_client(&cli, Provider::OpenAi).expect("build oauth client");
    let ptr = Arc::as_ptr(&client);
    assert!(!ptr.is_null());
}

#[test]
fn integration_build_provider_client_supports_openai_session_token_from_credential_store() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("credentials.json");
    write_test_provider_credential(
        &store_path,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::SessionToken,
            access_token: Some("openai-session-access".to_string()),
            refresh_token: None,
            expires_unix: Some(current_unix_timestamp().saturating_add(900)),
            revoked: false,
        },
    );

    let mut cli = test_cli();
    cli.openai_auth_mode = CliProviderAuthMode::SessionToken;
    cli.credential_store = store_path;
    cli.credential_store_encryption = CliCredentialStoreEncryptionMode::None;

    let client = build_provider_client(&cli, Provider::OpenAi).expect("build session client");
    let ptr = Arc::as_ptr(&client);
    assert!(!ptr.is_null());
}

#[test]
fn unit_resolve_credential_store_encryption_mode_auto_uses_key_presence() {
    let mut cli = test_cli();
    cli.credential_store_encryption = CliCredentialStoreEncryptionMode::Auto;
    cli.credential_store_key = None;
    assert_eq!(
        resolve_credential_store_encryption_mode(&cli),
        CredentialStoreEncryptionMode::None
    );

    cli.credential_store_key = Some("configured-key".to_string());
    assert_eq!(
        resolve_credential_store_encryption_mode(&cli),
        CredentialStoreEncryptionMode::Keyed
    );
}

#[test]
fn unit_resolve_prompt_input_uses_inline_prompt() {
    let mut cli = test_cli();
    cli.prompt = Some("inline prompt".to_string());

    let prompt = resolve_prompt_input(&cli).expect("resolve prompt");
    assert_eq!(prompt.as_deref(), Some("inline prompt"));
}

#[test]
fn unit_ensure_non_empty_text_returns_original_content() {
    let text = ensure_non_empty_text("hello".to_string(), "prompt".to_string())
        .expect("non-empty text should pass");
    assert_eq!(text, "hello");
}

#[test]
fn regression_ensure_non_empty_text_rejects_blank_content() {
    let error = ensure_non_empty_text(" \n\t".to_string(), "prompt".to_string())
        .expect_err("blank text should fail");
    assert!(error.to_string().contains("prompt is empty"));
}

#[test]
fn unit_parse_command_splits_name_and_args_with_extra_whitespace() {
    let parsed = parse_command("   /branch    42   ").expect("parse command");
    assert_eq!(parsed.name, "/branch");
    assert_eq!(parsed.args, "42");
}

#[test]
fn regression_parse_command_rejects_non_slash_input() {
    assert!(parse_command("help").is_none());
}

#[test]
fn unit_parse_session_search_args_supports_query_role_and_limit() {
    assert_eq!(
        parse_session_search_args("  retry budget  ").expect("parse query"),
        SessionSearchArgs {
            query: "retry budget".to_string(),
            role: None,
            limit: SESSION_SEARCH_DEFAULT_RESULTS,
        }
    );
    assert_eq!(
        parse_session_search_args("target --role user --limit 5").expect("parse flags"),
        SessionSearchArgs {
            query: "target".to_string(),
            role: Some("user".to_string()),
            limit: 5,
        }
    );
    assert_eq!(
        parse_session_search_args("--role=assistant --limit=9 delta").expect("parse inline"),
        SessionSearchArgs {
            query: "delta".to_string(),
            role: Some("assistant".to_string()),
            limit: 9,
        }
    );
}

#[test]
fn regression_parse_session_search_args_rejects_invalid_role_limit_and_flags() {
    let empty = parse_session_search_args(" \n\t ").expect_err("empty query should fail");
    assert!(empty.to_string().contains("query is required"));

    let invalid_role =
        parse_session_search_args("retry --role owner").expect_err("invalid role should fail");
    assert!(invalid_role.to_string().contains("invalid role"));

    let invalid_limit =
        parse_session_search_args("retry --limit 0").expect_err("limit zero should fail");
    assert!(invalid_limit
        .to_string()
        .contains("limit must be greater than 0"));

    let too_large =
        parse_session_search_args("retry --limit 9999").expect_err("too large limit should fail");
    assert!(too_large.to_string().contains("exceeds maximum"));

    let missing_role =
        parse_session_search_args("retry --role").expect_err("missing role value should fail");
    assert!(missing_role
        .to_string()
        .contains("missing value for --role"));

    let unknown_flag =
        parse_session_search_args("retry --unknown").expect_err("unknown flag should fail");
    assert!(unknown_flag.to_string().contains("unknown flag"));
}

#[test]
fn unit_session_message_preview_normalizes_whitespace_and_truncates() {
    let message = Message::user(format!(
        "line one\nline two {}",
        "x".repeat(SESSION_SEARCH_PREVIEW_CHARS)
    ));
    let preview = session_message_preview(&message);
    assert!(preview.starts_with("line one line two"));
    assert!(preview.ends_with("..."));
}

#[test]
fn unit_search_session_entries_matches_role_and_text_case_insensitively() {
    let entries = vec![
        crate::session::SessionEntry {
            id: 2,
            parent_id: Some(1),
            message: Message::assistant_text("Budget stabilized"),
        },
        crate::session::SessionEntry {
            id: 1,
            parent_id: None,
            message: Message::user("Root question"),
        },
    ];

    let (role_matches, role_total) = search_session_entries(&entries, "USER", None, 10);
    assert_eq!(role_total, 1);
    assert_eq!(role_matches[0].id, 1);
    assert_eq!(role_matches[0].role, "user");

    let (text_matches, text_total) = search_session_entries(&entries, "budget", None, 10);
    assert_eq!(text_total, 1);
    assert_eq!(text_matches[0].id, 2);
    assert_eq!(text_matches[0].role, "assistant");

    let (assistant_only, assistant_total) =
        search_session_entries(&entries, "budget", Some("assistant"), 10);
    assert_eq!(assistant_total, 1);
    assert_eq!(assistant_only[0].id, 2);
}

#[test]
fn functional_execute_session_search_command_renders_result_rows() {
    let temp = tempdir().expect("tempdir");
    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let head = store
        .append_messages(None, &[Message::system("sys")])
        .expect("append root");
    let head = store
        .append_messages(head, &[Message::user("Retry budget fix in progress")])
        .expect("append user");
    let runtime = SessionRuntime {
        store,
        active_head: head,
    };

    let output = execute_session_search_command(&runtime, "retry");
    assert!(output.contains("session search: query=\"retry\" role=any"));
    assert!(output.contains("matches=1"));
    assert!(output.contains("shown=1"));
    assert!(output.contains("limit=50"));
    assert!(output.contains("result: id=2 parent=1 role=user"));
    assert!(output.contains("preview=Retry budget fix in progress"));
}

#[test]
fn regression_search_session_entries_caps_huge_result_sets() {
    let entries = (1..=200)
        .map(|id| crate::session::SessionEntry {
            id,
            parent_id: if id == 1 { None } else { Some(id - 1) },
            message: Message::user(format!("needle-{id}")),
        })
        .collect::<Vec<_>>();
    let (matches, total_matches) =
        search_session_entries(&entries, "needle", None, SESSION_SEARCH_DEFAULT_RESULTS);
    assert_eq!(total_matches, 200);
    assert_eq!(matches.len(), SESSION_SEARCH_DEFAULT_RESULTS);
    assert_eq!(matches[0].id, 1);
    assert_eq!(
        matches.last().map(|item| item.id),
        Some(SESSION_SEARCH_DEFAULT_RESULTS as u64)
    );
}

#[test]
fn integration_execute_session_search_command_scans_entries_across_branches() {
    let temp = tempdir().expect("tempdir");
    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[Message::system("sys")])
        .expect("append root");
    let main_head = store
        .append_messages(root, &[Message::user("main branch target")])
        .expect("append main");
    let _branch_head = store
        .append_messages(root, &[Message::user("branch target")])
        .expect("append branch");
    let runtime = SessionRuntime {
        store,
        active_head: main_head,
    };

    let output = execute_session_search_command(&runtime, "target");
    let main_index = output.find("result: id=2").expect("main result");
    let branch_index = output.find("result: id=3").expect("branch result");
    assert!(main_index < branch_index);
}

#[test]
fn integration_execute_session_search_command_applies_role_filter_and_limit() {
    let temp = tempdir().expect("tempdir");
    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[Message::system("root target")])
        .expect("append root");
    let user_id = store
        .append_messages(root, &[Message::user("target user message")])
        .expect("append user");
    let _assistant_id = store
        .append_messages(
            user_id,
            &[Message::assistant_text("target assistant message")],
        )
        .expect("append assistant");
    let _tool_id = store
        .append_messages(
            user_id,
            &[Message::tool_result(
                "tool-call-1",
                "tool_call",
                "{}",
                false,
            )],
        )
        .expect("append tool");
    let runtime = SessionRuntime {
        store,
        active_head: user_id,
    };

    let output = execute_session_search_command(&runtime, "target --role user --limit 1");
    assert!(output.contains("role=user"));
    assert!(output.contains("matches=1"));
    assert!(output.contains("shown=1"));
    assert!(output.contains("limit=1"));
    assert!(output.contains("result: id=2 parent=1 role=user"));
    assert!(!output.contains("role=assistant"));
    assert!(!output.contains("role=tool"));
}

#[test]
fn unit_parse_session_diff_args_supports_default_and_explicit_heads() {
    assert_eq!(parse_session_diff_args("").expect("default heads"), None);
    assert_eq!(
        parse_session_diff_args(" 12  24 ").expect("explicit heads"),
        Some((12, 24))
    );
}

#[test]
fn regression_parse_session_diff_args_rejects_invalid_shapes() {
    let usage = parse_session_diff_args("12").expect_err("single head should fail");
    assert!(usage
        .to_string()
        .contains("usage: /session-diff [<left-id> <right-id>]"));

    let left_error = parse_session_diff_args("left 2").expect_err("invalid left head");
    assert!(left_error
        .to_string()
        .contains("invalid left session id 'left'"));
}

#[test]
fn unit_shared_lineage_prefix_depth_returns_common_ancestor_depth() {
    let left = vec![
        crate::session::SessionEntry {
            id: 1,
            parent_id: None,
            message: Message::system("root"),
        },
        crate::session::SessionEntry {
            id: 2,
            parent_id: Some(1),
            message: Message::user("shared"),
        },
        crate::session::SessionEntry {
            id: 4,
            parent_id: Some(2),
            message: Message::assistant_text("left"),
        },
    ];
    let right = vec![
        crate::session::SessionEntry {
            id: 1,
            parent_id: None,
            message: Message::system("root"),
        },
        crate::session::SessionEntry {
            id: 2,
            parent_id: Some(1),
            message: Message::user("shared"),
        },
        crate::session::SessionEntry {
            id: 5,
            parent_id: Some(2),
            message: Message::assistant_text("right"),
        },
    ];

    assert_eq!(shared_lineage_prefix_depth(&left, &right), 2);
}

#[test]
fn functional_render_session_diff_includes_summary_and_lineage_rows() {
    let report = SessionDiffReport {
        source: "explicit",
        left_id: 4,
        right_id: 5,
        shared_depth: 2,
        left_depth: 3,
        right_depth: 3,
        shared_entries: vec![SessionDiffEntry {
            id: 1,
            parent_id: None,
            role: "system".to_string(),
            preview: "root".to_string(),
        }],
        left_only_entries: vec![SessionDiffEntry {
            id: 4,
            parent_id: Some(2),
            role: "assistant".to_string(),
            preview: "left path".to_string(),
        }],
        right_only_entries: vec![SessionDiffEntry {
            id: 5,
            parent_id: Some(2),
            role: "assistant".to_string(),
            preview: "right path".to_string(),
        }],
    };

    let output = render_session_diff(&report);
    assert!(output.contains("session diff: source=explicit left=4 right=5"));
    assert!(output
        .contains("summary: shared_depth=2 left_depth=3 right_depth=3 left_only=1 right_only=1"));
    assert!(output.contains("shared: id=1 parent=none role=system preview=root"));
    assert!(output.contains("left-only: id=4 parent=2 role=assistant preview=left path"));
    assert!(output.contains("right-only: id=5 parent=2 role=assistant preview=right path"));
}

#[test]
fn integration_execute_session_diff_command_defaults_to_active_and_latest_heads() {
    let temp = tempdir().expect("tempdir");
    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let main_head = store
        .append_messages(Some(root), &[Message::user("main user")])
        .expect("append main")
        .expect("main head");
    let latest_head = store
        .append_messages(Some(root), &[Message::user("branch user")])
        .expect("append branch")
        .expect("branch head");
    let runtime = SessionRuntime {
        store,
        active_head: Some(main_head),
    };

    let output = execute_session_diff_command(&runtime, None);
    assert!(output.contains(&format!(
        "session diff: source=default left={} right={}",
        main_head, latest_head
    )));
    assert!(output
        .contains("summary: shared_depth=1 left_depth=2 right_depth=2 left_only=1 right_only=1"));
    assert!(output.contains("shared: id=1 parent=none role=system preview=sys"));
    assert!(output.contains("left-only: id=2 parent=1 role=user preview=main user"));
    assert!(output.contains("right-only: id=3 parent=1 role=user preview=branch user"));
}

#[test]
fn integration_execute_session_diff_command_supports_explicit_identical_heads() {
    let temp = tempdir().expect("tempdir");
    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let head = store
        .append_messages(Some(root), &[Message::user("user")])
        .expect("append user")
        .expect("head id");
    let runtime = SessionRuntime {
        store,
        active_head: Some(head),
    };

    let output = execute_session_diff_command(&runtime, Some((head, head)));
    assert!(output.contains("summary: shared_depth=2 left_depth=2 right_depth=2"));
    assert!(output.contains("left-only: none"));
    assert!(output.contains("right-only: none"));
}

#[test]
fn regression_execute_session_diff_command_reports_unknown_ids() {
    let temp = tempdir().expect("tempdir");
    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[Message::system("sys")])
        .expect("append")
        .expect("root");
    let runtime = SessionRuntime {
        store,
        active_head: Some(root),
    };

    let output = execute_session_diff_command(&runtime, Some((999, root)));
    assert!(output.contains("session diff error: unknown left session id 999"));
}

#[test]
fn regression_execute_session_diff_command_reports_empty_session_default_heads() {
    let temp = tempdir().expect("tempdir");
    let store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let runtime = SessionRuntime {
        store,
        active_head: None,
    };

    let output = execute_session_diff_command(&runtime, None);
    assert!(output.contains("session diff error: active head is not set"));
}

#[test]
fn regression_execute_session_diff_command_reports_malformed_graph() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("malformed-session.jsonl");
    let raw = [
        serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
        serde_json::json!({
            "record_type":"entry",
            "id":1,
            "parent_id":2,
            "message": Message::system("orphan")
        })
        .to_string(),
    ]
    .join("\n");
    std::fs::write(&session_path, format!("{raw}\n")).expect("write session");
    let store = SessionStore::load(&session_path).expect("load session");
    let runtime = SessionRuntime {
        store,
        active_head: Some(1),
    };

    let output = execute_session_diff_command(&runtime, None);
    assert!(output.contains("session diff error: unknown session id 2"));
}

#[test]
fn unit_compute_session_entry_depths_calculates_branch_depths() {
    let entries = vec![
        crate::session::SessionEntry {
            id: 3,
            parent_id: Some(2),
            message: Message::assistant_text("leaf"),
        },
        crate::session::SessionEntry {
            id: 1,
            parent_id: None,
            message: Message::system("root"),
        },
        crate::session::SessionEntry {
            id: 2,
            parent_id: Some(1),
            message: Message::user("middle"),
        },
    ];
    let depths = compute_session_entry_depths(&entries).expect("depth computation");
    assert_eq!(depths.get(&1), Some(&1));
    assert_eq!(depths.get(&2), Some(&2));
    assert_eq!(depths.get(&3), Some(&3));
}

#[test]
fn unit_compute_session_stats_calculates_core_counts() {
    let temp = tempdir().expect("tempdir");
    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let active_head = store
        .append_messages(Some(root), &[Message::user("user one")])
        .expect("append user")
        .expect("active head");
    let runtime = SessionRuntime {
        store,
        active_head: Some(active_head),
    };

    let stats = compute_session_stats(&runtime).expect("compute stats");
    assert_eq!(stats.entries, 2);
    assert_eq!(stats.branch_tips, 1);
    assert_eq!(stats.roots, 1);
    assert_eq!(stats.max_depth, 2);
    assert_eq!(stats.active_depth, Some(2));
    assert_eq!(stats.latest_depth, Some(2));
    assert!(stats.active_is_latest);
    assert_eq!(stats.role_counts.get("system"), Some(&1));
    assert_eq!(stats.role_counts.get("user"), Some(&1));
}

#[test]
fn functional_render_session_stats_includes_heads_depths_and_roles() {
    let mut role_counts = BTreeMap::new();
    role_counts.insert("assistant".to_string(), 2);
    role_counts.insert("user".to_string(), 1);
    let stats = SessionStats {
        entries: 3,
        branch_tips: 1,
        roots: 1,
        max_depth: 3,
        active_depth: Some(3),
        latest_depth: Some(3),
        active_head: Some(3),
        latest_head: Some(3),
        active_is_latest: true,
        role_counts,
    };

    let rendered = render_session_stats(&stats);
    assert!(rendered.contains("session stats: entries=3 branch_tips=1 roots=1 max_depth=3"));
    assert!(rendered.contains("heads: active=3 latest=3 active_is_latest=true"));
    assert!(rendered.contains("depth: active=3 latest=3"));
    assert!(rendered.contains("role: assistant=2"));
    assert!(rendered.contains("role: user=1"));
}

#[test]
fn unit_parse_session_stats_args_supports_default_and_json_modes() {
    assert_eq!(
        parse_session_stats_args("").expect("empty args"),
        SessionStatsOutputFormat::Text
    );
    assert_eq!(
        parse_session_stats_args("--json").expect("json flag"),
        SessionStatsOutputFormat::Json
    );
    let error = parse_session_stats_args("--bad").expect_err("invalid flag should fail");
    assert!(error.to_string().contains("usage: /session-stats [--json]"));
}

#[test]
fn unit_render_session_stats_json_includes_counts_and_roles() {
    let mut role_counts = BTreeMap::new();
    role_counts.insert("assistant".to_string(), 2);
    role_counts.insert("user".to_string(), 1);
    let stats = SessionStats {
        entries: 3,
        branch_tips: 1,
        roots: 1,
        max_depth: 3,
        active_depth: Some(3),
        latest_depth: Some(3),
        active_head: Some(3),
        latest_head: Some(3),
        active_is_latest: true,
        role_counts,
    };

    let json = render_session_stats_json(&stats);
    let value = serde_json::from_str::<serde_json::Value>(&json).expect("parse json");
    assert_eq!(value["entries"], 3);
    assert_eq!(value["branch_tips"], 1);
    assert_eq!(value["active_head"], 3);
    assert_eq!(value["role_counts"]["assistant"], 2);
    assert_eq!(value["role_counts"]["user"], 1);
}

#[test]
fn integration_execute_session_stats_command_summarizes_branched_session() {
    let temp = tempdir().expect("tempdir");
    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let main_head = store
        .append_messages(Some(root), &[Message::user("main user")])
        .expect("append main")
        .expect("main head");
    let branch_head = store
        .append_messages(Some(root), &[Message::user("branch user")])
        .expect("append branch")
        .expect("branch head");
    let latest_head = store
        .append_messages(
            Some(branch_head),
            &[Message::assistant_text("branch assistant")],
        )
        .expect("append branch assistant")
        .expect("latest head");
    let runtime = SessionRuntime {
        store,
        active_head: Some(main_head),
    };

    let output = execute_session_stats_command(&runtime, SessionStatsOutputFormat::Text);
    assert!(output.contains("session stats: entries=4"));
    assert!(output.contains("branch_tips=2"));
    assert!(output.contains("roots=1"));
    assert!(output.contains("max_depth=3"));
    assert!(output.contains(&format!(
        "heads: active={} latest={} active_is_latest=false",
        main_head, latest_head
    )));
    assert!(output.contains("role: assistant=1"));
    assert!(output.contains("role: system=1"));
    assert!(output.contains("role: user=2"));

    let json_output = execute_session_stats_command(&runtime, SessionStatsOutputFormat::Json);
    let value = serde_json::from_str::<serde_json::Value>(&json_output).expect("parse json");
    assert_eq!(value["entries"], 4);
    assert_eq!(value["branch_tips"], 2);
    assert_eq!(value["roots"], 1);
    assert_eq!(value["max_depth"], 3);
    assert_eq!(value["active_head"], main_head);
    assert_eq!(value["latest_head"], latest_head);
    assert_eq!(value["role_counts"]["assistant"], 1);
    assert_eq!(value["role_counts"]["system"], 1);
    assert_eq!(value["role_counts"]["user"], 2);
}

#[test]
fn regression_execute_session_stats_command_handles_empty_session() {
    let temp = tempdir().expect("tempdir");
    let store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let runtime = SessionRuntime {
        store,
        active_head: None,
    };

    let output = execute_session_stats_command(&runtime, SessionStatsOutputFormat::Text);
    assert!(output.contains("session stats: entries=0 branch_tips=0 roots=0 max_depth=0"));
    assert!(output.contains("heads: active=none latest=none active_is_latest=true"));
    assert!(output.contains("depth: active=none latest=none"));
    assert!(output.contains("roles: none"));

    let json_output = execute_session_stats_command(&runtime, SessionStatsOutputFormat::Json);
    let value = serde_json::from_str::<serde_json::Value>(&json_output).expect("parse json");
    assert_eq!(value["entries"], 0);
    assert_eq!(value["branch_tips"], 0);
    assert_eq!(value["roots"], 0);
    assert_eq!(value["max_depth"], 0);
    assert_eq!(value["active_head"], serde_json::Value::Null);
}

#[test]
fn regression_execute_session_stats_command_reports_malformed_graph() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("malformed-session.jsonl");
    let raw = [
        serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
        serde_json::json!({
            "record_type":"entry",
            "id":1,
            "parent_id":2,
            "message": Message::system("orphan")
        })
        .to_string(),
    ]
    .join("\n");
    std::fs::write(&session_path, format!("{raw}\n")).expect("write session");
    let store = SessionStore::load(&session_path).expect("load session");
    let runtime = SessionRuntime {
        store,
        active_head: Some(1),
    };

    let output = execute_session_stats_command(&runtime, SessionStatsOutputFormat::Text);
    assert!(output.contains("session stats error:"));
    assert!(output.contains("missing parent id 2"));

    let json_output = execute_session_stats_command(&runtime, SessionStatsOutputFormat::Json);
    let value = serde_json::from_str::<serde_json::Value>(&json_output).expect("parse json error");
    assert!(value["error"]
        .as_str()
        .expect("error string")
        .contains("missing parent id 2"));
}

#[test]
fn unit_build_doctor_command_config_collects_sorted_unique_provider_states() {
    let mut cli = test_cli();
    cli.no_session = true;
    cli.session = PathBuf::from("/tmp/session.jsonl");
    cli.skills_dir = PathBuf::from("/tmp/skills");
    cli.skills_lock_file = Some(PathBuf::from("/tmp/custom.lock.json"));
    cli.skill_trust_root_file = Some(PathBuf::from("/tmp/trust-roots.json"));
    cli.openai_api_key = Some("openai-key".to_string());
    cli.anthropic_api_key = Some("anthropic-key".to_string());
    cli.google_api_key = None;

    let primary = ModelRef {
        provider: Provider::OpenAi,
        model: "gpt-4o-mini".to_string(),
    };
    let fallbacks = vec![
        ModelRef {
            provider: Provider::Google,
            model: "gemini-2.5-pro".to_string(),
        },
        ModelRef {
            provider: Provider::Anthropic,
            model: "claude-sonnet-4".to_string(),
        },
        ModelRef {
            provider: Provider::OpenAi,
            model: "gpt-4.1-mini".to_string(),
        },
    ];
    let lock_path = PathBuf::from("/tmp/skills.lock.json");

    let config = build_doctor_command_config(&cli, &primary, &fallbacks, &lock_path);
    assert_eq!(config.model, "openai/gpt-4o-mini");
    assert!(!config.session_enabled);
    assert_eq!(config.session_path, PathBuf::from("/tmp/session.jsonl"));
    assert_eq!(config.skills_dir, PathBuf::from("/tmp/skills"));
    assert_eq!(config.skills_lock_path, lock_path);
    assert_eq!(
        config.trust_root_path,
        Some(PathBuf::from("/tmp/trust-roots.json"))
    );

    let provider_rows = config
        .provider_keys
        .iter()
        .map(|item| {
            (
                item.provider.clone(),
                item.key_env_var.clone(),
                item.present,
                item.auth_mode.as_str().to_string(),
                item.mode_supported,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        provider_rows,
        vec![
            (
                "anthropic".to_string(),
                "ANTHROPIC_API_KEY".to_string(),
                true,
                "api_key".to_string(),
                true
            ),
            (
                "google".to_string(),
                "GEMINI_API_KEY".to_string(),
                false,
                "api_key".to_string(),
                true
            ),
            (
                "openai".to_string(),
                "OPENAI_API_KEY".to_string(),
                true,
                "api_key".to_string(),
                true
            ),
        ]
    );
}

#[test]
fn unit_render_doctor_report_summarizes_counts_and_rows() {
    let report = render_doctor_report(&[
        DoctorCheckResult {
            key: "model".to_string(),
            status: DoctorStatus::Pass,
            code: "openai/gpt-4o-mini".to_string(),
            path: None,
            action: None,
        },
        DoctorCheckResult {
            key: "provider_key.openai".to_string(),
            status: DoctorStatus::Fail,
            code: "missing".to_string(),
            path: None,
            action: Some("set OPENAI_API_KEY".to_string()),
        },
        DoctorCheckResult {
            key: "skills_lock".to_string(),
            status: DoctorStatus::Warn,
            code: "missing".to_string(),
            path: Some("/tmp/skills.lock.json".to_string()),
            action: Some("run /skills-lock-write to generate lockfile".to_string()),
        },
    ]);

    assert!(report.contains("doctor summary: checks=3 pass=1 warn=1 fail=1"));
    assert!(report.contains(
        "doctor check: key=model status=pass code=openai/gpt-4o-mini path=none action=none"
    ));
    assert!(report.contains(
            "doctor check: key=provider_key.openai status=fail code=missing path=none action=set OPENAI_API_KEY"
        ));
    assert!(report.contains("doctor check: key=skills_lock status=warn code=missing path=/tmp/skills.lock.json action=run /skills-lock-write to generate lockfile"));
}

#[test]
fn unit_parse_doctor_command_args_supports_default_and_json_modes() {
    assert_eq!(
        parse_doctor_command_args("").expect("parse empty"),
        DoctorCommandOutputFormat::Text
    );
    assert_eq!(
        parse_doctor_command_args("--json").expect("parse json"),
        DoctorCommandOutputFormat::Json
    );

    let error = parse_doctor_command_args("--json --extra").expect_err("extra args should fail");
    assert!(error.to_string().contains("usage: /doctor [--json]"));
}

#[test]
fn unit_render_doctor_report_json_contains_summary_and_rows() {
    let report = render_doctor_report_json(&[
        DoctorCheckResult {
            key: "model".to_string(),
            status: DoctorStatus::Pass,
            code: "openai/gpt-4o-mini".to_string(),
            path: None,
            action: None,
        },
        DoctorCheckResult {
            key: "provider_key.openai".to_string(),
            status: DoctorStatus::Fail,
            code: "missing".to_string(),
            path: None,
            action: Some("set OPENAI_API_KEY".to_string()),
        },
    ]);
    let value = serde_json::from_str::<serde_json::Value>(&report).expect("parse json");
    assert_eq!(value["summary"]["checks"], 2);
    assert_eq!(value["summary"]["pass"], 1);
    assert_eq!(value["summary"]["warn"], 0);
    assert_eq!(value["summary"]["fail"], 1);
    assert_eq!(value["checks"][0]["key"], "model");
    assert_eq!(value["checks"][0]["status"], "pass");
    assert_eq!(value["checks"][1]["key"], "provider_key.openai");
    assert_eq!(value["checks"][1]["status"], "fail");
}

#[test]
fn functional_execute_doctor_command_supports_text_and_json_modes() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session.jsonl");
    let skills_dir = temp.path().join("skills");
    let lock_path = temp.path().join("skills.lock.json");
    let trust_root_path = temp.path().join("trust-roots.json");
    std::fs::create_dir_all(&skills_dir).expect("mkdir skills");
    std::fs::write(&session_path, "{}\n").expect("write session");
    std::fs::write(&lock_path, "{}\n").expect("write lock");
    std::fs::write(&trust_root_path, "[]\n").expect("write trust");

    let config = DoctorCommandConfig {
        model: "openai/gpt-4o-mini".to_string(),
        provider_keys: vec![
            DoctorProviderKeyStatus {
                provider_kind: Provider::Anthropic,
                provider: "anthropic".to_string(),
                key_env_var: "ANTHROPIC_API_KEY".to_string(),
                present: false,
                auth_mode: ProviderAuthMethod::ApiKey,
                mode_supported: true,
                login_backend_enabled: false,
                login_backend_executable: None,
                login_backend_available: false,
            },
            DoctorProviderKeyStatus {
                provider_kind: Provider::OpenAi,
                provider: "openai".to_string(),
                key_env_var: "OPENAI_API_KEY".to_string(),
                present: true,
                auth_mode: ProviderAuthMethod::ApiKey,
                mode_supported: true,
                login_backend_enabled: false,
                login_backend_executable: None,
                login_backend_available: false,
            },
        ],
        session_enabled: true,
        session_path,
        skills_dir,
        skills_lock_path: lock_path,
        trust_root_path: Some(trust_root_path),
    };

    let report = execute_doctor_command(&config, DoctorCommandOutputFormat::Text);
    assert!(report.contains("doctor summary: checks=9 pass=8 warn=0 fail=1"));

    let keys = report
        .lines()
        .skip(1)
        .map(|line| {
            line.split("key=")
                .nth(1)
                .expect("key section")
                .split(" status=")
                .next()
                .expect("key value")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        keys,
        vec![
            "model".to_string(),
            "provider_auth_mode.anthropic".to_string(),
            "provider_key.anthropic".to_string(),
            "provider_auth_mode.openai".to_string(),
            "provider_key.openai".to_string(),
            "session_path".to_string(),
            "skills_dir".to_string(),
            "skills_lock".to_string(),
            "trust_root".to_string(),
        ]
    );

    let json_report = execute_doctor_command(&config, DoctorCommandOutputFormat::Json);
    let value = serde_json::from_str::<serde_json::Value>(&json_report).expect("parse json report");
    assert_eq!(value["summary"]["checks"], 9);
    assert_eq!(value["summary"]["pass"], 8);
    assert_eq!(value["summary"]["warn"], 0);
    assert_eq!(value["summary"]["fail"], 1);
    assert_eq!(value["checks"][0]["key"], "model");
    assert_eq!(value["checks"][1]["key"], "provider_auth_mode.anthropic");
}

#[test]
fn integration_run_doctor_checks_identifies_missing_runtime_prerequisites() {
    let temp = tempdir().expect("tempdir");
    let config = DoctorCommandConfig {
        model: "openai/gpt-4o-mini".to_string(),
        provider_keys: vec![DoctorProviderKeyStatus {
            provider_kind: Provider::OpenAi,
            provider: "openai".to_string(),
            key_env_var: "OPENAI_API_KEY".to_string(),
            present: false,
            auth_mode: ProviderAuthMethod::ApiKey,
            mode_supported: true,
            login_backend_enabled: false,
            login_backend_executable: None,
            login_backend_available: false,
        }],
        session_enabled: true,
        session_path: temp.path().join("missing-parent").join("session.jsonl"),
        skills_dir: temp.path().join("missing-skills"),
        skills_lock_path: temp.path().join("missing-lock.json"),
        trust_root_path: Some(temp.path().join("missing-trust-roots.json")),
    };

    let checks = run_doctor_checks(&config);
    let by_key = checks
        .into_iter()
        .map(|check| (check.key.clone(), check))
        .collect::<HashMap<_, _>>();

    assert_eq!(
        by_key.get("model").map(|item| item.status),
        Some(DoctorStatus::Pass)
    );
    assert_eq!(
        by_key
            .get("provider_auth_mode.openai")
            .map(|item| (item.status, item.code.clone())),
        Some((DoctorStatus::Pass, "api_key".to_string()))
    );
    assert_eq!(
        by_key
            .get("provider_key.openai")
            .map(|item| (item.status, item.code.clone())),
        Some((DoctorStatus::Fail, "missing".to_string()))
    );
    assert_eq!(
        by_key
            .get("session_path")
            .map(|item| (item.status, item.code.clone())),
        Some((DoctorStatus::Fail, "missing_parent".to_string()))
    );
    assert_eq!(
        by_key
            .get("skills_dir")
            .map(|item| (item.status, item.code.clone())),
        Some((DoctorStatus::Warn, "missing".to_string()))
    );
    assert_eq!(
        by_key
            .get("skills_lock")
            .map(|item| (item.status, item.code.clone())),
        Some((DoctorStatus::Warn, "missing".to_string()))
    );
    assert_eq!(
        by_key
            .get("trust_root")
            .map(|item| (item.status, item.code.clone())),
        Some((DoctorStatus::Warn, "missing".to_string()))
    );
}

#[test]
fn integration_run_doctor_checks_reports_google_backend_status_for_oauth_mode() {
    let temp = tempdir().expect("tempdir");
    let config = DoctorCommandConfig {
        model: "google/gemini-2.5-pro".to_string(),
        provider_keys: vec![DoctorProviderKeyStatus {
            provider_kind: Provider::Google,
            provider: "google".to_string(),
            key_env_var: "GEMINI_API_KEY".to_string(),
            present: false,
            auth_mode: ProviderAuthMethod::OauthToken,
            mode_supported: true,
            login_backend_enabled: false,
            login_backend_executable: Some("gemini".to_string()),
            login_backend_available: false,
        }],
        session_enabled: false,
        session_path: temp.path().join("session.jsonl"),
        skills_dir: temp.path().join("skills"),
        skills_lock_path: temp.path().join("skills.lock.json"),
        trust_root_path: None,
    };

    let checks = run_doctor_checks(&config);
    let by_key = checks
        .into_iter()
        .map(|check| (check.key.clone(), check))
        .collect::<HashMap<_, _>>();

    assert_eq!(
        by_key
            .get("provider_auth_mode.google")
            .map(|item| (item.status, item.code.clone())),
        Some((DoctorStatus::Pass, "oauth_token".to_string()))
    );
    assert_eq!(
        by_key
            .get("provider_backend.google")
            .map(|item| (item.status, item.code.clone())),
        Some((DoctorStatus::Fail, "backend_disabled".to_string()))
    );
}

#[test]
fn integration_run_doctor_checks_reports_anthropic_backend_status_for_oauth_mode() {
    let temp = tempdir().expect("tempdir");
    let config = DoctorCommandConfig {
        model: "anthropic/claude-sonnet-4-20250514".to_string(),
        provider_keys: vec![DoctorProviderKeyStatus {
            provider_kind: Provider::Anthropic,
            provider: "anthropic".to_string(),
            key_env_var: "ANTHROPIC_API_KEY".to_string(),
            present: false,
            auth_mode: ProviderAuthMethod::OauthToken,
            mode_supported: true,
            login_backend_enabled: false,
            login_backend_executable: Some("claude".to_string()),
            login_backend_available: false,
        }],
        session_enabled: false,
        session_path: temp.path().join("session.jsonl"),
        skills_dir: temp.path().join("skills"),
        skills_lock_path: temp.path().join("skills.lock.json"),
        trust_root_path: None,
    };

    let checks = run_doctor_checks(&config);
    let by_key = checks
        .into_iter()
        .map(|check| (check.key.clone(), check))
        .collect::<HashMap<_, _>>();

    assert_eq!(
        by_key
            .get("provider_auth_mode.anthropic")
            .map(|item| (item.status, item.code.clone())),
        Some((DoctorStatus::Pass, "oauth_token".to_string()))
    );
    assert_eq!(
        by_key
            .get("provider_backend.anthropic")
            .map(|item| (item.status, item.code.clone())),
        Some((DoctorStatus::Fail, "backend_disabled".to_string()))
    );
}

#[test]
fn integration_doctor_command_preserves_session_runtime() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write skill");
    let lock_path = default_skills_lock_path(&skills_dir);
    std::fs::write(&lock_path, "{}\n").expect("write lock");
    let trust_root_path = temp.path().join("trust-roots.json");
    std::fs::write(&trust_root_path, "[]\n").expect("write trust");

    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let head = store
        .append_messages(Some(root), &[tau_ai::Message::user("hello")])
        .expect("append user")
        .expect("head id");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = store.lineage_messages(Some(head)).expect("lineage");
    agent.replace_messages(lineage.clone());

    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let auth_command_config = test_auth_command_config();
    let mut skills_command_config =
        skills_command_config(&skills_dir, &lock_path, Some(&trust_root_path));
    skills_command_config.doctor_config.session_path = temp.path().join("session.jsonl");

    let action = handle_command_with_session_import_mode(
        "/doctor",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &auth_command_config,
        &ModelCatalog::built_in(),
        &[],
    )
    .expect("doctor command should continue");
    assert_eq!(action, CommandAction::Continue);

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.active_head, Some(head));
    assert_eq!(runtime.store.entries().len(), 2);
    assert_eq!(agent.messages().len(), lineage.len());
}

#[test]
fn regression_run_doctor_checks_reports_type_and_readability_errors() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session-as-dir");
    std::fs::create_dir_all(&session_path).expect("mkdir session dir");
    let skills_dir = temp.path().join("skills-as-file");
    std::fs::write(&skills_dir, "not a directory").expect("write skills file");
    let lock_path = temp.path().join("lock-as-dir");
    std::fs::create_dir_all(&lock_path).expect("mkdir lock dir");
    let trust_root_path = temp.path().join("trust-as-dir");
    std::fs::create_dir_all(&trust_root_path).expect("mkdir trust dir");

    let config = DoctorCommandConfig {
        model: "openai/gpt-4o-mini".to_string(),
        provider_keys: vec![DoctorProviderKeyStatus {
            provider_kind: Provider::OpenAi,
            provider: "openai".to_string(),
            key_env_var: "OPENAI_API_KEY".to_string(),
            present: true,
            auth_mode: ProviderAuthMethod::ApiKey,
            mode_supported: true,
            login_backend_enabled: false,
            login_backend_executable: None,
            login_backend_available: false,
        }],
        session_enabled: true,
        session_path,
        skills_dir,
        skills_lock_path: lock_path,
        trust_root_path: Some(trust_root_path),
    };

    let checks = run_doctor_checks(&config);
    let by_key = checks
        .into_iter()
        .map(|check| (check.key.clone(), check))
        .collect::<HashMap<_, _>>();

    assert_eq!(
        by_key
            .get("provider_auth_mode.openai")
            .map(|item| (item.status, item.code.clone())),
        Some((DoctorStatus::Pass, "api_key".to_string()))
    );

    assert_eq!(
        by_key
            .get("session_path")
            .map(|item| (item.status, item.code.clone())),
        Some((DoctorStatus::Fail, "not_file".to_string()))
    );
    assert_eq!(
        by_key
            .get("skills_dir")
            .map(|item| (item.status, item.code.clone())),
        Some((DoctorStatus::Fail, "not_dir".to_string()))
    );
    let lock = by_key.get("skills_lock").expect("skills lock check");
    assert_eq!(lock.status, DoctorStatus::Fail);
    assert!(lock.code.starts_with("read_error:"));
    let trust = by_key.get("trust_root").expect("trust root check");
    assert_eq!(trust.status, DoctorStatus::Fail);
    assert!(trust.code.starts_with("read_error:"));
}

#[test]
fn unit_resolve_session_graph_format_and_escape_label_behaviors() {
    assert_eq!(
        resolve_session_graph_format(Path::new("/tmp/graph.dot")),
        SessionGraphFormat::Dot
    );
    assert_eq!(
        resolve_session_graph_format(Path::new("/tmp/graph.mmd")),
        SessionGraphFormat::Mermaid
    );
    assert_eq!(escape_graph_label("a\"b\\c"), "a\\\"b\\\\c".to_string());
}

#[test]
fn unit_render_session_graph_mermaid_and_dot_include_deterministic_edges() {
    let entries = vec![
        crate::session::SessionEntry {
            id: 2,
            parent_id: Some(1),
            message: Message::user("child"),
        },
        crate::session::SessionEntry {
            id: 1,
            parent_id: None,
            message: Message::system("root"),
        },
    ];

    let mermaid = render_session_graph_mermaid(&entries);
    assert!(mermaid.contains("graph TD"));
    let root_index = mermaid.find("n1[\"1: system | root\"]").expect("root node");
    let child_index = mermaid.find("n2[\"2: user | child\"]").expect("child node");
    assert!(root_index < child_index);
    assert!(mermaid.contains("n1 --> n2"));

    let dot = render_session_graph_dot(&entries);
    assert!(dot.contains("digraph session"));
    assert!(dot.contains("n1 [label=\"1: system | root\"];"));
    assert!(dot.contains("n2 [label=\"2: user | child\"];"));
    assert!(dot.contains("n1 -> n2;"));
}

#[test]
fn functional_execute_session_graph_export_command_writes_mermaid_file() {
    let temp = tempdir().expect("tempdir");
    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[Message::system("root")])
        .expect("append root")
        .expect("root id");
    let _head = store
        .append_messages(Some(root), &[Message::user("child")])
        .expect("append child")
        .expect("head id");
    let runtime = SessionRuntime {
        store,
        active_head: Some(root + 1),
    };
    let destination = temp.path().join("session-graph.mmd");

    let output =
        execute_session_graph_export_command(&runtime, destination.to_str().expect("utf8 path"));
    assert!(output.contains("session graph export: path="));
    assert!(output.contains("format=mermaid"));
    assert!(output.contains("nodes=2"));
    assert!(output.contains("edges=1"));

    let raw = std::fs::read_to_string(destination).expect("read graph");
    assert!(raw.contains("graph TD"));
    assert!(raw.contains("n1 --> n2"));
}

#[test]
fn integration_execute_session_graph_export_command_supports_dot_for_branched_session() {
    let temp = tempdir().expect("tempdir");
    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[Message::system("root")])
        .expect("append root")
        .expect("root id");
    let _main = store
        .append_messages(Some(root), &[Message::user("main")])
        .expect("append main")
        .expect("main id");
    let _branch = store
        .append_messages(Some(root), &[Message::user("branch")])
        .expect("append branch")
        .expect("branch id");
    let runtime = SessionRuntime {
        store,
        active_head: Some(root + 2),
    };
    let destination = temp.path().join("session-graph.dot");

    let output =
        execute_session_graph_export_command(&runtime, destination.to_str().expect("utf8 path"));
    assert!(output.contains("format=dot"));
    assert!(output.contains("nodes=3"));
    assert!(output.contains("edges=2"));

    let raw = std::fs::read_to_string(destination).expect("read graph");
    assert!(raw.contains("digraph session"));
    assert!(raw.contains("n1 -> n2;"));
    assert!(raw.contains("n1 -> n3;"));
}

#[test]
fn regression_execute_session_graph_export_command_rejects_directory_destination() {
    let temp = tempdir().expect("tempdir");
    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[Message::system("root")])
        .expect("append root")
        .expect("root id");
    let runtime = SessionRuntime {
        store,
        active_head: Some(root),
    };
    let destination_dir = temp.path().join("graph-dir");
    std::fs::create_dir_all(&destination_dir).expect("mkdir");

    let output = execute_session_graph_export_command(
        &runtime,
        destination_dir.to_str().expect("utf8 path"),
    );
    assert!(output.contains("session graph export error: path="));
    assert!(output.contains("is a directory"));
}

#[test]
fn unit_default_macro_config_path_uses_project_local_file_location() {
    let path = default_macro_config_path().expect("resolve macro path");
    assert!(path.ends_with(Path::new(".tau").join("macros.json")));
}

#[test]
fn unit_validate_macro_name_accepts_and_rejects_expected_inputs() {
    validate_macro_name("quick_check").expect("valid macro name");

    let error = validate_macro_name("").expect_err("empty macro name should fail");
    assert!(error.to_string().contains("must not be empty"));

    let error =
        validate_macro_name("9check").expect_err("macro name starting with digit should fail");
    assert!(error
        .to_string()
        .contains("must start with an ASCII letter"));

    let error = validate_macro_name("check.list")
        .expect_err("macro name with unsupported punctuation should fail");
    assert!(error
        .to_string()
        .contains("must contain only ASCII letters, digits, '-' or '_'"));
}

#[test]
fn functional_parse_macro_command_supports_lifecycle_and_usage_rules() {
    assert_eq!(
        parse_macro_command("list").expect("parse list"),
        MacroCommand::List
    );
    assert_eq!(
        parse_macro_command("save quick /tmp/quick.commands").expect("parse save"),
        MacroCommand::Save {
            name: "quick".to_string(),
            commands_file: PathBuf::from("/tmp/quick.commands"),
        }
    );
    assert_eq!(
        parse_macro_command("run quick").expect("parse run"),
        MacroCommand::Run {
            name: "quick".to_string(),
            dry_run: false,
        }
    );
    assert_eq!(
        parse_macro_command("run quick --dry-run").expect("parse dry run"),
        MacroCommand::Run {
            name: "quick".to_string(),
            dry_run: true,
        }
    );
    assert_eq!(
        parse_macro_command("show quick").expect("parse show"),
        MacroCommand::Show {
            name: "quick".to_string(),
        }
    );
    assert_eq!(
        parse_macro_command("delete quick").expect("parse delete"),
        MacroCommand::Delete {
            name: "quick".to_string(),
        }
    );

    let error = parse_macro_command("").expect_err("missing args should fail");
    assert!(error.to_string().contains(MACRO_USAGE));

    let error = parse_macro_command("run quick --apply").expect_err("unknown run flag should fail");
    assert!(error
        .to_string()
        .contains("usage: /macro run <name> [--dry-run]"));

    let error =
        parse_macro_command("list extra").expect_err("list with extra arguments should fail");
    assert!(error.to_string().contains("usage: /macro list"));

    let error = parse_macro_command("show").expect_err("show without name should fail");
    assert!(error.to_string().contains("usage: /macro show <name>"));
}

#[test]
fn unit_validate_macro_command_entry_rejects_nested_unknown_and_exit_commands() {
    validate_macro_command_entry("/session").expect("known command should validate");

    let error =
        validate_macro_command_entry("session").expect_err("command without slash should fail");
    assert!(error.to_string().contains("must start with '/'"));

    let error =
        validate_macro_command_entry("/does-not-exist").expect_err("unknown command should fail");
    assert!(error
        .to_string()
        .contains("unknown command '/does-not-exist'"));

    let error =
        validate_macro_command_entry("/macro list").expect_err("nested macro command should fail");
    assert!(error
        .to_string()
        .contains("nested /macro commands are not allowed"));

    let error = validate_macro_command_entry("/quit").expect_err("exit commands should fail");
    assert!(error.to_string().contains("exit commands are not allowed"));
}

#[test]
fn unit_save_and_load_macro_file_round_trip_schema_and_values() {
    let temp = tempdir().expect("tempdir");
    let macro_path = temp.path().join(".tau").join("macros.json");
    let macros = BTreeMap::from([
        (
            "quick".to_string(),
            vec!["/session".to_string(), "/session-stats".to_string()],
        ),
        ("review".to_string(), vec!["/help session".to_string()]),
    ]);

    save_macro_file(&macro_path, &macros).expect("save macro file");

    let loaded = load_macro_file(&macro_path).expect("load macro file");
    assert_eq!(loaded, macros);

    let raw = std::fs::read_to_string(&macro_path).expect("read macro file");
    let parsed = serde_json::from_str::<MacroFile>(&raw).expect("parse macro file");
    assert_eq!(parsed.schema_version, MACRO_SCHEMA_VERSION);
    assert_eq!(parsed.macros, macros);
}

#[test]
fn functional_render_macro_helpers_support_empty_and_deterministic_order() {
    let path = Path::new("/tmp/macros.json");
    let empty = render_macro_list(path, &BTreeMap::new());
    assert!(empty.contains("count=0"));
    assert!(empty.contains("macros: none"));

    let macros = BTreeMap::from([
        ("beta".to_string(), vec!["/session".to_string()]),
        (
            "alpha".to_string(),
            vec!["/session".to_string(), "/session-stats".to_string()],
        ),
    ]);
    let output = render_macro_list(path, &macros);
    let alpha_index = output.find("macro: name=alpha").expect("alpha row");
    let beta_index = output.find("macro: name=beta").expect("beta row");
    assert!(alpha_index < beta_index);

    let show = render_macro_show(path, "alpha", macros.get("alpha").expect("alpha commands"));
    assert!(show.contains("macro show: path=/tmp/macros.json name=alpha commands=2"));
    assert!(show.contains("command: index=0 value=/session"));
    assert!(show.contains("command: index=1 value=/session-stats"));
}

#[test]
fn integration_execute_macro_command_save_show_run_delete_lifecycle() {
    let temp = tempdir().expect("tempdir");
    let macro_path = temp.path().join(".tau").join("macros.json");
    let commands_file = temp.path().join("rewind.commands");
    std::fs::write(&commands_file, "/branch 1\n/session\n").expect("write commands file");

    let session_path = temp.path().join("session.jsonl");
    let mut store = SessionStore::load(&session_path).expect("load");
    let root = store
        .append_messages(None, &[Message::system("root")])
        .expect("append root")
        .expect("root id");
    let head = store
        .append_messages(Some(root), &[Message::assistant_text("leaf")])
        .expect("append leaf")
        .expect("head id");
    let mut session_runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = session_runtime
        .as_ref()
        .expect("runtime")
        .store
        .lineage_messages(Some(head))
        .expect("lineage");
    agent.replace_messages(lineage);

    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let auth_command_config = test_auth_command_config();
    let model_catalog = ModelCatalog::built_in();
    let skills_dir = temp.path().join("skills");
    let lock_path = default_skills_lock_path(&skills_dir);
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);
    let command_context = CommandExecutionContext {
        tool_policy_json: &tool_policy_json,
        session_import_mode: SessionImportMode::Merge,
        profile_defaults: &profile_defaults,
        skills_command_config: &skills_command_config,
        auth_command_config: &auth_command_config,
        model_catalog: &model_catalog,
        extension_commands: &[],
    };

    let save_output = execute_macro_command(
        &format!("save rewind {}", commands_file.display()),
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(save_output.contains("macro save: path="));
    assert!(save_output.contains("name=rewind"));
    assert!(save_output.contains("commands=2"));

    let dry_run_output = execute_macro_command(
        "run rewind --dry-run",
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(dry_run_output.contains("mode=dry-run"));
    assert!(dry_run_output.contains("plan: command=/branch 1"));
    assert_eq!(
        session_runtime
            .as_ref()
            .and_then(|runtime| runtime.active_head),
        Some(head)
    );

    let show_output = execute_macro_command(
        "show rewind",
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(show_output.contains("macro show: path="));
    assert!(show_output.contains("name=rewind commands=2"));
    assert!(show_output.contains("command: index=0 value=/branch 1"));
    assert!(show_output.contains("command: index=1 value=/session"));

    let run_output = execute_macro_command(
        "run rewind",
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(run_output.contains("macro run: path="));
    assert!(run_output.contains("mode=apply"));
    assert!(run_output.contains("executed=2"));
    assert_eq!(
        session_runtime
            .as_ref()
            .and_then(|runtime| runtime.active_head),
        Some(root)
    );

    let list_output = execute_macro_command(
        "list",
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(list_output.contains("macro list: path="));
    assert!(list_output.contains("count=1"));
    assert!(list_output.contains("macro: name=rewind commands=2"));

    let delete_output = execute_macro_command(
        "delete rewind",
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(delete_output.contains("macro delete: path="));
    assert!(delete_output.contains("name=rewind"));
    assert!(delete_output.contains("status=deleted"));
    assert!(delete_output.contains("remaining=0"));

    let final_list = execute_macro_command(
        "list",
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(final_list.contains("count=0"));
    assert!(final_list.contains("macros: none"));
}

#[test]
fn regression_execute_macro_command_reports_missing_commands_file() {
    let temp = tempdir().expect("tempdir");
    let macro_path = temp.path().join(".tau").join("macros.json");
    let missing_commands_file = temp.path().join("missing.commands");
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let auth_command_config = test_auth_command_config();
    let model_catalog = ModelCatalog::built_in();
    let skills_dir = temp.path().join("skills");
    let lock_path = default_skills_lock_path(&skills_dir);
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);
    let command_context = CommandExecutionContext {
        tool_policy_json: &tool_policy_json,
        session_import_mode: SessionImportMode::Merge,
        profile_defaults: &profile_defaults,
        skills_command_config: &skills_command_config,
        auth_command_config: &auth_command_config,
        model_catalog: &model_catalog,
        extension_commands: &[],
    };
    let mut session_runtime = None;
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());

    let output = execute_macro_command(
        &format!("save quick {}", missing_commands_file.display()),
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(output.contains("macro error: path="));
    assert!(output.contains("failed to read commands file"));
}

#[test]
fn regression_execute_macro_command_reports_corrupt_macro_file() {
    let temp = tempdir().expect("tempdir");
    let macro_path = temp.path().join(".tau").join("macros.json");
    std::fs::create_dir_all(
        macro_path
            .parent()
            .expect("macro path should include a parent"),
    )
    .expect("create macro config dir");
    std::fs::write(&macro_path, "{invalid-json").expect("write malformed macro file");

    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let auth_command_config = test_auth_command_config();
    let model_catalog = ModelCatalog::built_in();
    let skills_dir = temp.path().join("skills");
    let lock_path = default_skills_lock_path(&skills_dir);
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);
    let command_context = CommandExecutionContext {
        tool_policy_json: &tool_policy_json,
        session_import_mode: SessionImportMode::Merge,
        profile_defaults: &profile_defaults,
        skills_command_config: &skills_command_config,
        auth_command_config: &auth_command_config,
        model_catalog: &model_catalog,
        extension_commands: &[],
    };
    let mut session_runtime = None;
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());

    let output = execute_macro_command(
        "list",
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(output.contains("macro error: path="));
    assert!(output.contains("failed to parse macro file"));
}

#[test]
fn regression_execute_macro_command_rejects_unknown_macro_and_invalid_entries() {
    let temp = tempdir().expect("tempdir");
    let macro_path = temp.path().join(".tau").join("macros.json");
    let macros = BTreeMap::from([("broken".to_string(), vec!["/macro list".to_string()])]);
    save_macro_file(&macro_path, &macros).expect("save macro file");

    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let auth_command_config = test_auth_command_config();
    let model_catalog = ModelCatalog::built_in();
    let skills_dir = temp.path().join("skills");
    let lock_path = default_skills_lock_path(&skills_dir);
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);
    let command_context = CommandExecutionContext {
        tool_policy_json: &tool_policy_json,
        session_import_mode: SessionImportMode::Merge,
        profile_defaults: &profile_defaults,
        skills_command_config: &skills_command_config,
        auth_command_config: &auth_command_config,
        model_catalog: &model_catalog,
        extension_commands: &[],
    };
    let mut session_runtime = None;
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());

    let missing_output = execute_macro_command(
        "run missing",
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(missing_output.contains("unknown macro 'missing'"));

    let missing_show = execute_macro_command(
        "show missing",
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(missing_show.contains("unknown macro 'missing'"));

    let missing_delete = execute_macro_command(
        "delete missing",
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(missing_delete.contains("unknown macro 'missing'"));

    let invalid_output = execute_macro_command(
        "run broken",
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(invalid_output.contains("macro command #0 failed validation"));

    let delete_broken = execute_macro_command(
        "delete broken",
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(delete_broken.contains("status=deleted"));
    assert!(delete_broken.contains("remaining=0"));
}

#[test]
fn unit_validate_profile_name_accepts_and_rejects_expected_inputs() {
    validate_profile_name("baseline_1").expect("valid profile name");

    let error = validate_profile_name("").expect_err("empty profile name should fail");
    assert!(error.to_string().contains("must not be empty"));

    let error = validate_profile_name("1baseline")
        .expect_err("profile name starting with digit should fail");
    assert!(error
        .to_string()
        .contains("must start with an ASCII letter"));

    let error = validate_profile_name("baseline.json")
        .expect_err("profile name with punctuation should fail");
    assert!(error
        .to_string()
        .contains("must contain only ASCII letters, digits, '-' or '_'"));
}

#[test]
fn functional_parse_profile_command_supports_lifecycle_subcommands_and_usage_errors() {
    assert_eq!(
        parse_profile_command("save baseline").expect("parse save"),
        ProfileCommand::Save {
            name: "baseline".to_string(),
        }
    );
    assert_eq!(
        parse_profile_command("load baseline").expect("parse load"),
        ProfileCommand::Load {
            name: "baseline".to_string(),
        }
    );
    assert_eq!(
        parse_profile_command("list").expect("parse list"),
        ProfileCommand::List
    );
    assert_eq!(
        parse_profile_command("show baseline").expect("parse show"),
        ProfileCommand::Show {
            name: "baseline".to_string(),
        }
    );
    assert_eq!(
        parse_profile_command("delete baseline").expect("parse delete"),
        ProfileCommand::Delete {
            name: "baseline".to_string(),
        }
    );

    let error = parse_profile_command("").expect_err("empty args should fail");
    assert!(error.to_string().contains(PROFILE_USAGE));

    let error = parse_profile_command("save").expect_err("missing name should fail");
    assert!(error.to_string().contains("usage: /profile save <name>"));

    let error =
        parse_profile_command("list extra").expect_err("list with trailing arguments should fail");
    assert!(error.to_string().contains("usage: /profile list"));

    let error = parse_profile_command("show").expect_err("show missing name should fail");
    assert!(error.to_string().contains("usage: /profile show <name>"));

    let error =
        parse_profile_command("unknown baseline").expect_err("unknown subcommand should fail");
    assert!(error.to_string().contains("unknown subcommand 'unknown'"));
}

#[test]
fn unit_save_and_load_profile_store_round_trip_schema_and_values() {
    let temp = tempdir().expect("tempdir");
    let profile_path = temp.path().join(".tau").join("profiles.json");
    let mut alternate = test_profile_defaults();
    alternate.model = "google/gemini-2.5-pro".to_string();
    let profiles = BTreeMap::from([
        ("baseline".to_string(), test_profile_defaults()),
        ("alt".to_string(), alternate.clone()),
    ]);

    save_profile_store(&profile_path, &profiles).expect("save profiles");
    let loaded = load_profile_store(&profile_path).expect("load profiles");
    assert_eq!(loaded, profiles);

    let raw = std::fs::read_to_string(&profile_path).expect("read profile file");
    let parsed = serde_json::from_str::<ProfileStoreFile>(&raw).expect("parse profile file");
    assert_eq!(parsed.schema_version, PROFILE_SCHEMA_VERSION);
    assert_eq!(parsed.profiles, profiles);
}

#[test]
fn regression_load_profile_store_backfills_auth_defaults_for_legacy_profiles() {
    let temp = tempdir().expect("tempdir");
    let profile_path = temp.path().join(".tau").join("profiles.json");
    std::fs::create_dir_all(
        profile_path
            .parent()
            .expect("profile path should have parent"),
    )
    .expect("mkdir profile dir");
    std::fs::write(
        &profile_path,
        serde_json::json!({
            "schema_version": PROFILE_SCHEMA_VERSION,
            "profiles": {
                "legacy": {
                    "model": "openai/gpt-4o-mini",
                    "fallback_models": [],
                    "session": {
                        "enabled": true,
                        "path": ".tau/sessions/default.jsonl",
                        "import_mode": "merge"
                    },
                    "policy": {
                        "tool_policy_preset": "balanced",
                        "bash_profile": "balanced",
                        "bash_dry_run": false,
                        "os_sandbox_mode": "off",
                        "enforce_regular_files": true,
                        "bash_timeout_ms": 500,
                        "max_command_length": 4096,
                        "max_tool_output_bytes": 1024,
                        "max_file_read_bytes": 2048,
                        "max_file_write_bytes": 2048,
                        "allow_command_newlines": true
                    }
                }
            }
        })
        .to_string(),
    )
    .expect("write legacy profile store");

    let loaded = load_profile_store(&profile_path).expect("load legacy profiles");
    let legacy = loaded.get("legacy").expect("legacy profile");
    assert_eq!(legacy.auth.openai, ProviderAuthMethod::ApiKey);
    assert_eq!(legacy.auth.anthropic, ProviderAuthMethod::ApiKey);
    assert_eq!(legacy.auth.google, ProviderAuthMethod::ApiKey);
}

#[test]
fn functional_render_profile_diffs_reports_changed_fields() {
    let current = test_profile_defaults();
    let mut loaded = current.clone();
    loaded.model = "google/gemini-2.5-pro".to_string();
    loaded.policy.max_command_length = 2048;
    loaded.session.import_mode = "replace".to_string();

    let diffs = render_profile_diffs(&current, &loaded);
    assert_eq!(diffs.len(), 3);
    assert!(diffs
        .iter()
        .any(|line| line
            .contains("field=model current=openai/gpt-4o-mini loaded=google/gemini-2.5-pro")));
    assert!(diffs
        .iter()
        .any(|line| line.contains("field=session.import_mode current=merge loaded=replace")));
    assert!(diffs
        .iter()
        .any(|line| line.contains("field=policy.max_command_length current=4096 loaded=2048")));
}

#[test]
fn functional_render_profile_diffs_reports_changed_auth_modes() {
    let current = test_profile_defaults();
    let mut loaded = current.clone();
    loaded.auth.openai = ProviderAuthMethod::OauthToken;
    loaded.auth.google = ProviderAuthMethod::Adc;

    let diffs = render_profile_diffs(&current, &loaded);
    assert!(diffs
        .iter()
        .any(|line| line.contains("field=auth.openai current=api_key loaded=oauth_token")));
    assert!(diffs
        .iter()
        .any(|line| line.contains("field=auth.google current=api_key loaded=adc")));
}

#[test]
fn unit_render_profile_list_and_show_produce_deterministic_output() {
    let profile_path = PathBuf::from("/tmp/profiles.json");
    let mut alternate = test_profile_defaults();
    alternate.model = "google/gemini-2.5-pro".to_string();
    let profiles = BTreeMap::from([
        ("zeta".to_string(), test_profile_defaults()),
        ("alpha".to_string(), alternate.clone()),
    ]);

    let list_output = render_profile_list(&profile_path, &profiles);
    assert!(list_output.contains("profile list: path=/tmp/profiles.json profiles=2"));
    let alpha_index = list_output.find("profile: name=alpha").expect("alpha row");
    let zeta_index = list_output.find("profile: name=zeta").expect("zeta row");
    assert!(alpha_index < zeta_index);

    let show_output = render_profile_show(&profile_path, "alpha", &alternate);
    assert!(show_output.contains("profile show: path=/tmp/profiles.json name=alpha status=found"));
    assert!(show_output.contains("value: model=google/gemini-2.5-pro"));
    assert!(show_output.contains("value: fallback_models=none"));
    assert!(show_output.contains("value: session.path=.tau/sessions/default.jsonl"));
    assert!(show_output.contains("value: policy.max_command_length=4096"));
    assert!(show_output.contains("value: auth.openai=api_key"));
}

#[test]
fn integration_execute_profile_command_full_lifecycle_roundtrip() {
    let temp = tempdir().expect("tempdir");
    let profile_path = temp.path().join(".tau").join("profiles.json");
    let current = test_profile_defaults();

    let save_output = execute_profile_command("save baseline", &profile_path, &current);
    assert!(save_output.contains("profile save: path="));
    assert!(save_output.contains("name=baseline"));
    assert!(save_output.contains("status=saved"));

    let load_output = execute_profile_command("load baseline", &profile_path, &current);
    assert!(load_output.contains("profile load: path="));
    assert!(load_output.contains("name=baseline"));
    assert!(load_output.contains("status=in_sync"));
    assert!(load_output.contains("diffs=0"));

    let list_output = execute_profile_command("list", &profile_path, &current);
    assert!(list_output.contains("profile list: path="));
    assert!(list_output.contains("profiles=1"));
    assert!(list_output.contains("profile: name=baseline"));

    let show_output = execute_profile_command("show baseline", &profile_path, &current);
    assert!(show_output.contains("profile show: path="));
    assert!(show_output.contains("name=baseline status=found"));
    assert!(show_output.contains("value: model=openai/gpt-4o-mini"));

    let mut changed = current.clone();
    changed.model = "anthropic/claude-sonnet-4-20250514".to_string();
    let diff_output = execute_profile_command("load baseline", &profile_path, &changed);
    assert!(diff_output.contains("status=diff"));
    assert!(diff_output.contains("diff: field=model"));

    let delete_output = execute_profile_command("delete baseline", &profile_path, &current);
    assert!(delete_output.contains("profile delete: path="));
    assert!(delete_output.contains("name=baseline"));
    assert!(delete_output.contains("status=deleted"));
    assert!(delete_output.contains("remaining=0"));

    let list_after_delete = execute_profile_command("list", &profile_path, &current);
    assert!(list_after_delete.contains("profiles=0"));
    assert!(list_after_delete.contains("names=none"));
}

#[test]
fn regression_execute_profile_command_reports_unknown_profile_and_schema_errors() {
    let temp = tempdir().expect("tempdir");
    let profile_path = temp.path().join(".tau").join("profiles.json");
    let current = test_profile_defaults();

    let missing_output = execute_profile_command("load missing", &profile_path, &current);
    assert!(missing_output.contains("profile error: path="));
    assert!(missing_output.contains("unknown profile 'missing'"));

    let missing_show = execute_profile_command("show missing", &profile_path, &current);
    assert!(missing_show.contains("profile error: path="));
    assert!(missing_show.contains("unknown profile 'missing'"));

    let missing_delete = execute_profile_command("delete missing", &profile_path, &current);
    assert!(missing_delete.contains("profile error: path="));
    assert!(missing_delete.contains("unknown profile 'missing'"));

    std::fs::create_dir_all(
        profile_path
            .parent()
            .expect("profile path should include parent dir"),
    )
    .expect("create profile dir");
    let invalid = serde_json::json!({
        "schema_version": 999,
        "profiles": {
            "baseline": current
        }
    });
    std::fs::write(&profile_path, format!("{invalid}\n")).expect("write invalid schema");

    let schema_output = execute_profile_command("load baseline", &profile_path, &current);
    assert!(schema_output.contains("profile error: path="));
    assert!(schema_output.contains("unsupported profile schema_version 999"));
}

#[test]
fn regression_default_profile_store_path_uses_project_local_profiles_file() {
    let path = default_profile_store_path().expect("resolve profile store path");
    assert!(path.ends_with(Path::new(".tau").join("profiles.json")));
}

#[test]
fn unit_command_file_error_mode_label_matches_cli_values() {
    assert_eq!(
        command_file_error_mode_label(CliCommandFileErrorMode::FailFast),
        "fail-fast"
    );
    assert_eq!(
        command_file_error_mode_label(CliCommandFileErrorMode::ContinueOnError),
        "continue-on-error"
    );
}

#[test]
fn unit_parse_command_file_skips_comments_blanks_and_keeps_line_numbers() {
    let temp = tempdir().expect("tempdir");
    let command_file = temp.path().join("commands.txt");
    std::fs::write(
        &command_file,
        "# comment\n\n  /session  \nnot-command\n   # another comment\n/help session\n",
    )
    .expect("write command file");

    let entries = parse_command_file(&command_file).expect("parse command file");
    assert_eq!(entries.len(), 3);
    assert_eq!(
        entries[0],
        CommandFileEntry {
            line_number: 3,
            command: "/session".to_string(),
        }
    );
    assert_eq!(
        entries[1],
        CommandFileEntry {
            line_number: 4,
            command: "not-command".to_string(),
        }
    );
    assert_eq!(
        entries[2],
        CommandFileEntry {
            line_number: 6,
            command: "/help session".to_string(),
        }
    );
}

#[test]
fn functional_execute_command_file_runs_script_and_returns_summary() {
    let temp = tempdir().expect("tempdir");
    let command_file = temp.path().join("commands.txt");
    std::fs::write(&command_file, "/session\n/help session\n").expect("write command file");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let mut session_runtime = None;
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let auth_command_config = test_auth_command_config();
    let model_catalog = ModelCatalog::built_in();
    let skills_dir = temp.path().join("skills");
    let lock_path = default_skills_lock_path(&skills_dir);
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);
    let command_context = test_command_context(
        &tool_policy_json,
        &profile_defaults,
        &skills_command_config,
        &auth_command_config,
        &model_catalog,
    );

    let report = execute_command_file(
        &command_file,
        CliCommandFileErrorMode::FailFast,
        &mut agent,
        &mut session_runtime,
        command_context,
    )
    .expect("execute command file");

    assert_eq!(
        report,
        CommandFileReport {
            total: 2,
            executed: 2,
            succeeded: 2,
            failed: 0,
            halted_early: false,
        }
    );
}

#[test]
fn integration_execute_command_file_continue_on_error_runs_remaining_commands() {
    let temp = tempdir().expect("tempdir");
    let command_file = temp.path().join("commands.txt");
    std::fs::write(&command_file, "/session\nnot-command\n/help session\n")
        .expect("write command file");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let mut session_runtime = None;
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let auth_command_config = test_auth_command_config();
    let model_catalog = ModelCatalog::built_in();
    let skills_dir = temp.path().join("skills");
    let lock_path = default_skills_lock_path(&skills_dir);
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);
    let command_context = test_command_context(
        &tool_policy_json,
        &profile_defaults,
        &skills_command_config,
        &auth_command_config,
        &model_catalog,
    );

    let report = execute_command_file(
        &command_file,
        CliCommandFileErrorMode::ContinueOnError,
        &mut agent,
        &mut session_runtime,
        command_context,
    )
    .expect("execute command file");

    assert_eq!(
        report,
        CommandFileReport {
            total: 3,
            executed: 3,
            succeeded: 2,
            failed: 1,
            halted_early: false,
        }
    );
}

#[test]
fn regression_execute_command_file_fail_fast_stops_on_malformed_line() {
    let temp = tempdir().expect("tempdir");
    let command_file = temp.path().join("commands.txt");
    std::fs::write(&command_file, "/session\nnot-command\n/help session\n")
        .expect("write command file");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let mut session_runtime = None;
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let auth_command_config = test_auth_command_config();
    let model_catalog = ModelCatalog::built_in();
    let skills_dir = temp.path().join("skills");
    let lock_path = default_skills_lock_path(&skills_dir);
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);
    let command_context = test_command_context(
        &tool_policy_json,
        &profile_defaults,
        &skills_command_config,
        &auth_command_config,
        &model_catalog,
    );

    let error = execute_command_file(
        &command_file,
        CliCommandFileErrorMode::FailFast,
        &mut agent,
        &mut session_runtime,
        command_context,
    )
    .expect_err("fail-fast should stop on malformed command line");
    assert!(error.to_string().contains("command file execution failed"));
}

#[test]
fn regression_parse_command_file_reports_missing_file() {
    let temp = tempdir().expect("tempdir");
    let missing = temp.path().join("missing-commands.txt");
    let error = parse_command_file(&missing).expect_err("missing command file should fail");
    assert!(error.to_string().contains("failed to read command file"));
}

#[test]
fn unit_validate_branch_alias_name_accepts_and_rejects_expected_inputs() {
    validate_branch_alias_name("hotfix_1").expect("valid alias");

    let error = validate_branch_alias_name("").expect_err("empty alias should fail");
    assert!(error.to_string().contains("must not be empty"));

    let error =
        validate_branch_alias_name("1hotfix").expect_err("alias starting with a digit should fail");
    assert!(error
        .to_string()
        .contains("must start with an ASCII letter"));

    let error = validate_branch_alias_name("hotfix.bad")
        .expect_err("alias with unsupported punctuation should fail");
    assert!(error
        .to_string()
        .contains("must contain only ASCII letters, digits, '-' or '_'"));
}

#[test]
fn functional_parse_branch_alias_command_supports_core_subcommands() {
    assert_eq!(
        parse_branch_alias_command("list").expect("parse list"),
        BranchAliasCommand::List
    );
    assert_eq!(
        parse_branch_alias_command("set hotfix 42").expect("parse set"),
        BranchAliasCommand::Set {
            name: "hotfix".to_string(),
            id: 42,
        }
    );
    assert_eq!(
        parse_branch_alias_command("use hotfix").expect("parse use"),
        BranchAliasCommand::Use {
            name: "hotfix".to_string(),
        }
    );

    let error = parse_branch_alias_command("").expect_err("missing args should fail");
    assert!(error.to_string().contains(BRANCH_ALIAS_USAGE));

    let error = parse_branch_alias_command("set hotfix nope").expect_err("invalid id should fail");
    assert!(error.to_string().contains("invalid branch id 'nope'"));

    let error =
        parse_branch_alias_command("delete hotfix").expect_err("unknown subcommand should fail");
    assert!(error.to_string().contains("unknown subcommand 'delete'"));
}

#[test]
fn unit_save_and_load_branch_aliases_round_trip_schema_and_values() {
    let temp = tempdir().expect("tempdir");
    let alias_path = temp.path().join("session.aliases.json");
    let aliases = BTreeMap::from([
        ("hotfix".to_string(), 7_u64),
        ("rollback".to_string(), 12_u64),
    ]);

    save_branch_aliases(&alias_path, &aliases).expect("save aliases");

    let loaded = load_branch_aliases(&alias_path).expect("load aliases");
    assert_eq!(loaded, aliases);

    let raw = std::fs::read_to_string(&alias_path).expect("read alias file");
    let parsed = serde_json::from_str::<BranchAliasFile>(&raw).expect("parse alias file");
    assert_eq!(parsed.schema_version, BRANCH_ALIAS_SCHEMA_VERSION);
    assert_eq!(parsed.aliases, aliases);
}

#[test]
fn integration_execute_branch_alias_command_supports_set_use_and_list_flow() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session.jsonl");
    let mut store = SessionStore::load(&session_path).expect("load");
    let root = store
        .append_messages(None, &[Message::system("root")])
        .expect("append root")
        .expect("root id");
    let stable = store
        .append_messages(Some(root), &[Message::assistant_text("stable branch")])
        .expect("append stable")
        .expect("stable id");
    let hot = store
        .append_messages(Some(root), &[Message::assistant_text("hot branch")])
        .expect("append hot")
        .expect("hot id");
    let mut runtime = SessionRuntime {
        store,
        active_head: Some(hot),
    };
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = runtime
        .store
        .lineage_messages(runtime.active_head)
        .expect("lineage");
    agent.replace_messages(lineage);

    let set_output =
        execute_branch_alias_command(&format!("set hotfix {stable}"), &mut agent, &mut runtime);
    assert!(set_output.contains("branch alias set: path="));
    assert!(set_output.contains("name=hotfix"));
    assert_eq!(runtime.active_head, Some(hot));

    let list_output = execute_branch_alias_command("list", &mut agent, &mut runtime);
    assert!(list_output.contains("branch alias list: path="));
    assert!(list_output.contains("count=1"));
    assert!(list_output.contains(&format!("alias: name=hotfix id={} status=ok", stable)));

    let use_output = execute_branch_alias_command("use hotfix", &mut agent, &mut runtime);
    assert!(use_output.contains("branch alias use: path="));
    assert!(use_output.contains(&format!("id={stable}")));
    assert_eq!(runtime.active_head, Some(stable));

    let alias_path = branch_alias_path_for_session(&session_path);
    let aliases = load_branch_aliases(&alias_path).expect("load aliases");
    assert_eq!(aliases.get("hotfix"), Some(&stable));
}

#[test]
fn regression_execute_branch_alias_command_reports_stale_alias_ids() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session.jsonl");
    let mut store = SessionStore::load(&session_path).expect("load");
    let root = store
        .append_messages(None, &[Message::system("root")])
        .expect("append root")
        .expect("root id");
    let mut runtime = SessionRuntime {
        store,
        active_head: Some(root),
    };
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let alias_path = branch_alias_path_for_session(&session_path);
    let aliases = BTreeMap::from([("legacy".to_string(), 999_u64)]);
    save_branch_aliases(&alias_path, &aliases).expect("save stale alias");

    let list_output = execute_branch_alias_command("list", &mut agent, &mut runtime);
    assert!(list_output.contains("count=1"));
    assert!(list_output.contains("alias: name=legacy id=999 status=stale"));

    let use_output = execute_branch_alias_command("use legacy", &mut agent, &mut runtime);
    assert!(use_output.contains("branch alias error: path="));
    assert!(use_output.contains("alias points to unknown session id 999"));
}

#[test]
fn regression_execute_branch_alias_command_reports_corrupt_alias_file() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session.jsonl");
    let mut store = SessionStore::load(&session_path).expect("load");
    let root = store
        .append_messages(None, &[Message::system("root")])
        .expect("append root")
        .expect("root id");
    let mut runtime = SessionRuntime {
        store,
        active_head: Some(root),
    };
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let alias_path = branch_alias_path_for_session(&session_path);
    std::fs::write(&alias_path, "{invalid-json").expect("write malformed alias file");

    let output = execute_branch_alias_command("list", &mut agent, &mut runtime);
    assert!(output.contains("branch alias error: path="));
    assert!(output.contains("failed to parse alias file"));
}

#[test]
fn functional_parse_session_bookmark_command_supports_lifecycle_subcommands() {
    assert_eq!(
        parse_session_bookmark_command("list").expect("parse list"),
        SessionBookmarkCommand::List
    );
    assert_eq!(
        parse_session_bookmark_command("set checkpoint 42").expect("parse set"),
        SessionBookmarkCommand::Set {
            name: "checkpoint".to_string(),
            id: 42,
        }
    );
    assert_eq!(
        parse_session_bookmark_command("use checkpoint").expect("parse use"),
        SessionBookmarkCommand::Use {
            name: "checkpoint".to_string(),
        }
    );
    assert_eq!(
        parse_session_bookmark_command("delete checkpoint").expect("parse delete"),
        SessionBookmarkCommand::Delete {
            name: "checkpoint".to_string(),
        }
    );

    let error = parse_session_bookmark_command("").expect_err("empty args should fail");
    assert!(error.to_string().contains(SESSION_BOOKMARK_USAGE));

    let error =
        parse_session_bookmark_command("set checkpoint nope").expect_err("invalid id should fail");
    assert!(error.to_string().contains("invalid bookmark id 'nope'"));

    let error =
        parse_session_bookmark_command("unknown checkpoint").expect_err("unknown subcommand");
    assert!(error.to_string().contains("unknown subcommand 'unknown'"));
}

#[test]
fn unit_save_and_load_session_bookmarks_round_trip_schema_and_values() {
    let temp = tempdir().expect("tempdir");
    let bookmark_path = temp.path().join("session.bookmarks.json");
    let bookmarks = BTreeMap::from([
        ("checkpoint".to_string(), 7_u64),
        ("investigation".to_string(), 42_u64),
    ]);

    save_session_bookmarks(&bookmark_path, &bookmarks).expect("save bookmarks");
    let loaded = load_session_bookmarks(&bookmark_path).expect("load bookmarks");
    assert_eq!(loaded, bookmarks);

    let raw = std::fs::read_to_string(&bookmark_path).expect("read bookmark file");
    let parsed = serde_json::from_str::<SessionBookmarkFile>(&raw).expect("parse bookmark file");
    assert_eq!(parsed.schema_version, SESSION_BOOKMARK_SCHEMA_VERSION);
    assert_eq!(parsed.bookmarks, bookmarks);
}

#[test]
fn integration_execute_session_bookmark_command_supports_set_use_list_delete_flow() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session.jsonl");
    let mut store = SessionStore::load(&session_path).expect("load");
    let root = store
        .append_messages(None, &[Message::system("root")])
        .expect("append root")
        .expect("root id");
    let stable = store
        .append_messages(Some(root), &[Message::user("stable branch")])
        .expect("append stable branch")
        .expect("stable id");

    let mut runtime = SessionRuntime {
        store,
        active_head: Some(root),
    };
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let initial_lineage = runtime
        .store
        .lineage_messages(runtime.active_head)
        .expect("initial lineage");
    agent.replace_messages(initial_lineage);

    let set_output = execute_session_bookmark_command(
        &format!("set checkpoint {stable}"),
        &mut agent,
        &mut runtime,
    );
    assert!(set_output.contains("session bookmark set: path="));
    assert!(set_output.contains("name=checkpoint"));
    assert!(set_output.contains(&format!("id={stable}")));

    let list_output = execute_session_bookmark_command("list", &mut agent, &mut runtime);
    assert!(list_output.contains("session bookmark list: path="));
    assert!(list_output.contains("count=1"));
    assert!(list_output.contains(&format!("bookmark: name=checkpoint id={stable} status=ok")));

    let use_output = execute_session_bookmark_command("use checkpoint", &mut agent, &mut runtime);
    assert!(use_output.contains("session bookmark use: path="));
    assert!(use_output.contains(&format!("id={stable}")));
    assert_eq!(runtime.active_head, Some(stable));

    let delete_output =
        execute_session_bookmark_command("delete checkpoint", &mut agent, &mut runtime);
    assert!(delete_output.contains("session bookmark delete: path="));
    assert!(delete_output.contains("status=deleted"));
    assert!(delete_output.contains("remaining=0"));

    let final_list = execute_session_bookmark_command("list", &mut agent, &mut runtime);
    assert!(final_list.contains("count=0"));
    assert!(final_list.contains("bookmarks: none"));
}

#[test]
fn regression_execute_session_bookmark_command_reports_stale_ids() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session.jsonl");
    let mut store = SessionStore::load(&session_path).expect("load");
    let root = store
        .append_messages(None, &[Message::system("root")])
        .expect("append root")
        .expect("root id");
    let mut runtime = SessionRuntime {
        store,
        active_head: Some(root),
    };
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let bookmark_path = session_bookmark_path_for_session(&session_path);
    let bookmarks = BTreeMap::from([("legacy".to_string(), 999_u64)]);
    save_session_bookmarks(&bookmark_path, &bookmarks).expect("save stale bookmark");

    let list_output = execute_session_bookmark_command("list", &mut agent, &mut runtime);
    assert!(list_output.contains("count=1"));
    assert!(list_output.contains("bookmark: name=legacy id=999 status=stale"));

    let use_output = execute_session_bookmark_command("use legacy", &mut agent, &mut runtime);
    assert!(use_output.contains("session bookmark error: path="));
    assert!(use_output.contains("bookmark points to unknown session id 999"));
}

#[test]
fn regression_execute_session_bookmark_command_reports_corrupt_bookmark_file() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session.jsonl");
    let mut store = SessionStore::load(&session_path).expect("load");
    let root = store
        .append_messages(None, &[Message::system("root")])
        .expect("append root")
        .expect("root id");
    let mut runtime = SessionRuntime {
        store,
        active_head: Some(root),
    };
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let bookmark_path = session_bookmark_path_for_session(&session_path);
    std::fs::write(&bookmark_path, "{invalid-json").expect("write malformed bookmark file");

    let output = execute_session_bookmark_command("list", &mut agent, &mut runtime);
    assert!(output.contains("session bookmark error: path="));
    assert!(output.contains("failed to parse session bookmark file"));
}

#[test]
fn functional_render_help_overview_lists_known_commands() {
    let help = render_help_overview();
    assert!(help.contains("/help [command]"));
    assert!(help.contains("/session"));
    assert!(help.contains("/session-search <query> [--role <role>] [--limit <n>]"));
    assert!(help.contains("/session-stats"));
    assert!(help.contains("/session-diff [<left-id> <right-id>]"));
    assert!(help.contains("/doctor"));
    assert!(help.contains("/session-graph-export <path>"));
    assert!(help.contains("/session-export <path>"));
    assert!(help.contains("/session-import <path>"));
    assert!(help.contains("/audit-summary <path>"));
    assert!(help.contains(MODELS_LIST_USAGE));
    assert!(help.contains(MODEL_SHOW_USAGE));
    assert!(help.contains("/skills-search <query> [max_results]"));
    assert!(help.contains("/skills-show <name>"));
    assert!(help.contains("/skills-list"));
    assert!(help.contains("/skills-lock-diff [lockfile_path] [--json]"));
    assert!(help.contains("/skills-prune [lockfile_path] [--dry-run|--apply]"));
    assert!(help.contains("/skills-trust-list [trust_root_file]"));
    assert!(help.contains("/skills-trust-add <id=base64_key> [trust_root_file]"));
    assert!(help.contains("/skills-trust-revoke <id> [trust_root_file]"));
    assert!(help.contains("/skills-trust-rotate <old_id:new_id=base64_key> [trust_root_file]"));
    assert!(help.contains("/skills-verify [lockfile_path] [trust_root_file] [--json]"));
    assert!(help.contains("/skills-lock-write [lockfile_path]"));
    assert!(help.contains("/skills-sync [lockfile_path]"));
    assert!(help.contains("/macro <save|run|list|show|delete> ..."));
    assert!(help.contains("/auth <login|status|logout|matrix> ..."));
    assert!(help.contains("/integration-auth <set|status|rotate|revoke> ..."));
    assert!(help.contains("/profile <save|load|list|show|delete> ..."));
    assert!(help.contains("/branch <id>"));
    assert!(help.contains("/branch-alias <set|list|use> ..."));
    assert!(help.contains("/session-bookmark <set|list|use|delete> ..."));
    assert!(help.contains("/quit"));
}

#[test]
fn functional_render_command_help_supports_branch_topic_without_slash() {
    let help = render_command_help("branch").expect("render help");
    assert!(help.contains("command: /branch"));
    assert!(help.contains("usage: /branch <id>"));
    assert!(help.contains("example: /branch 12"));
}

#[test]
fn functional_render_command_help_supports_branch_alias_topic_without_slash() {
    let help = render_command_help("branch-alias").expect("render help");
    assert!(help.contains("command: /branch-alias"));
    assert!(help.contains("usage: /branch-alias <set|list|use> ..."));
    assert!(help.contains("example: /branch-alias set hotfix 42"));
}

#[test]
fn functional_render_command_help_supports_session_bookmark_topic_without_slash() {
    let help = render_command_help("session-bookmark").expect("render help");
    assert!(help.contains("command: /session-bookmark"));
    assert!(help.contains("usage: /session-bookmark <set|list|use|delete> ..."));
    assert!(help.contains("example: /session-bookmark set investigation 42"));
}

#[test]
fn functional_render_command_help_supports_macro_topic_without_slash() {
    let help = render_command_help("macro").expect("render help");
    assert!(help.contains("command: /macro"));
    assert!(help.contains("usage: /macro <save|run|list|show|delete> ..."));
    assert!(help.contains("example: /macro save quick-check /tmp/quick-check.commands"));
}

#[test]
fn functional_render_command_help_supports_integration_auth_topic_without_slash() {
    let help = render_command_help("integration-auth").expect("render help");
    assert!(help.contains("command: /integration-auth"));
    assert!(help.contains("usage: /integration-auth <set|status|rotate|revoke> ..."));
    assert!(help.contains("example: /integration-auth status github-token --json"));
}

#[test]
fn functional_render_command_help_supports_profile_topic_without_slash() {
    let help = render_command_help("profile").expect("render help");
    assert!(help.contains("command: /profile"));
    assert!(help.contains("usage: /profile <save|load|list|show|delete> ..."));
    assert!(help.contains("example: /profile save baseline"));
}

#[test]
fn functional_render_command_help_supports_session_search_topic_without_slash() {
    let help = render_command_help("session-search").expect("render help");
    assert!(help.contains("command: /session-search"));
    assert!(help.contains("usage: /session-search <query> [--role <role>] [--limit <n>]"));
}

#[test]
fn functional_render_command_help_supports_session_stats_topic_without_slash() {
    let help = render_command_help("session-stats").expect("render help");
    assert!(help.contains("command: /session-stats"));
    assert!(help.contains("usage: /session-stats [--json]"));
}

#[test]
fn functional_render_command_help_supports_session_diff_topic_without_slash() {
    let help = render_command_help("session-diff").expect("render help");
    assert!(help.contains("command: /session-diff"));
    assert!(help.contains("usage: /session-diff [<left-id> <right-id>]"));
}

#[test]
fn functional_render_command_help_supports_doctor_topic_without_slash() {
    let help = render_command_help("doctor").expect("render help");
    assert!(help.contains("command: /doctor"));
    assert!(help.contains("usage: /doctor [--json]"));
    assert!(help.contains("example: /doctor"));
}

#[test]
fn functional_render_command_help_supports_session_graph_export_topic_without_slash() {
    let help = render_command_help("session-graph-export").expect("render help");
    assert!(help.contains("command: /session-graph-export"));
    assert!(help.contains("usage: /session-graph-export <path>"));
}

#[test]
fn functional_render_command_help_supports_models_list_topic_without_slash() {
    let help = render_command_help("models-list").expect("render help");
    assert!(help.contains("command: /models-list"));
    assert!(help.contains(&format!("usage: {MODELS_LIST_USAGE}")));
}

#[test]
fn functional_render_command_help_supports_model_show_topic_without_slash() {
    let help = render_command_help("model-show").expect("render help");
    assert!(help.contains("command: /model-show"));
    assert!(help.contains(&format!("usage: {MODEL_SHOW_USAGE}")));
}

#[test]
fn functional_render_command_help_supports_skills_sync_topic_without_slash() {
    let help = render_command_help("skills-sync").expect("render help");
    assert!(help.contains("command: /skills-sync"));
    assert!(help.contains("usage: /skills-sync [lockfile_path]"));
}

#[test]
fn functional_render_command_help_supports_skills_lock_write_topic_without_slash() {
    let help = render_command_help("skills-lock-write").expect("render help");
    assert!(help.contains("command: /skills-lock-write"));
    assert!(help.contains("usage: /skills-lock-write [lockfile_path]"));
}

#[test]
fn functional_render_command_help_supports_skills_list_topic_without_slash() {
    let help = render_command_help("skills-list").expect("render help");
    assert!(help.contains("command: /skills-list"));
    assert!(help.contains("usage: /skills-list"));
}

#[test]
fn functional_render_command_help_supports_skills_show_topic_without_slash() {
    let help = render_command_help("skills-show").expect("render help");
    assert!(help.contains("command: /skills-show"));
    assert!(help.contains("usage: /skills-show <name>"));
}

#[test]
fn functional_render_command_help_supports_skills_search_topic_without_slash() {
    let help = render_command_help("skills-search").expect("render help");
    assert!(help.contains("command: /skills-search"));
    assert!(help.contains("usage: /skills-search <query> [max_results]"));
}

#[test]
fn functional_render_command_help_supports_skills_lock_diff_topic_without_slash() {
    let help = render_command_help("skills-lock-diff").expect("render help");
    assert!(help.contains("command: /skills-lock-diff"));
    assert!(help.contains("usage: /skills-lock-diff [lockfile_path] [--json]"));
}

#[test]
fn functional_render_command_help_supports_skills_prune_topic_without_slash() {
    let help = render_command_help("skills-prune").expect("render help");
    assert!(help.contains("command: /skills-prune"));
    assert!(help.contains("usage: /skills-prune [lockfile_path] [--dry-run|--apply]"));
}

#[test]
fn functional_render_command_help_supports_skills_trust_list_topic_without_slash() {
    let help = render_command_help("skills-trust-list").expect("render help");
    assert!(help.contains("command: /skills-trust-list"));
    assert!(help.contains("usage: /skills-trust-list [trust_root_file]"));
}

#[test]
fn functional_render_command_help_supports_skills_trust_add_topic_without_slash() {
    let help = render_command_help("skills-trust-add").expect("render help");
    assert!(help.contains("command: /skills-trust-add"));
    assert!(help.contains("usage: /skills-trust-add <id=base64_key> [trust_root_file]"));
}

#[test]
fn functional_render_command_help_supports_skills_trust_revoke_topic_without_slash() {
    let help = render_command_help("skills-trust-revoke").expect("render help");
    assert!(help.contains("command: /skills-trust-revoke"));
    assert!(help.contains("usage: /skills-trust-revoke <id> [trust_root_file]"));
}

#[test]
fn functional_render_command_help_supports_skills_trust_rotate_topic_without_slash() {
    let help = render_command_help("skills-trust-rotate").expect("render help");
    assert!(help.contains("command: /skills-trust-rotate"));
    assert!(
        help.contains("usage: /skills-trust-rotate <old_id:new_id=base64_key> [trust_root_file]")
    );
}

#[test]
fn functional_render_command_help_supports_skills_verify_topic_without_slash() {
    let help = render_command_help("skills-verify").expect("render help");
    assert!(help.contains("command: /skills-verify"));
    assert!(help.contains("usage: /skills-verify [lockfile_path] [trust_root_file] [--json]"));
}

#[test]
fn regression_unknown_command_message_suggests_closest_match() {
    let message = unknown_command_message("/polciy");
    assert!(message.contains("did you mean /policy?"));
}

#[test]
fn regression_unknown_command_message_without_close_match_has_no_suggestion() {
    let message = unknown_command_message("/zzzzzzzz");
    assert!(!message.contains("did you mean"));
}

#[test]
fn unit_format_id_list_renders_none_and_csv() {
    assert_eq!(format_id_list(&[]), "none");
    assert_eq!(format_id_list(&[1, 2, 42]), "1,2,42");
}

#[test]
fn unit_format_remap_ids_renders_none_and_pairs() {
    assert_eq!(format_remap_ids(&[]), "none");
    assert_eq!(format_remap_ids(&[(1, 3), (2, 4)]), "1->3,2->4");
}

#[test]
fn unit_resolve_skills_lock_path_uses_default_and_explicit_values() {
    let default_lock_path = PathBuf::from(".tau/skills/skills.lock.json");
    assert_eq!(
        resolve_skills_lock_path("", &default_lock_path),
        default_lock_path
    );
    assert_eq!(
        resolve_skills_lock_path("custom/lock.json", &default_lock_path),
        PathBuf::from("custom/lock.json")
    );
}

#[test]
fn unit_render_skills_sync_drift_details_uses_none_placeholders() {
    let report = crate::skills::SkillsSyncReport {
        expected_entries: 2,
        actual_entries: 2,
        ..crate::skills::SkillsSyncReport::default()
    };
    assert_eq!(
        render_skills_sync_drift_details(&report),
        "expected_entries=2 actual_entries=2 missing=none extra=none changed=none metadata=none"
    );
}

#[test]
fn unit_render_skills_lock_write_success_formats_path_and_entry_count() {
    let rendered = render_skills_lock_write_success(Path::new("skills.lock.json"), 3);
    assert_eq!(
        rendered,
        "skills lock write: path=skills.lock.json entries=3"
    );
}

#[test]
fn unit_render_skills_list_handles_empty_catalog() {
    let rendered = render_skills_list(Path::new(".tau/skills"), &[]);
    assert!(rendered.contains("skills list: path=.tau/skills count=0"));
    assert!(rendered.contains("skills: none"));
}

#[test]
fn unit_render_skills_show_includes_metadata_and_content() {
    let skill = crate::skills::Skill {
        name: "checklist".to_string(),
        content: "line one\nline two".to_string(),
        path: PathBuf::from("checklist.md"),
    };
    let rendered = render_skills_show(Path::new(".tau/skills"), &skill);
    assert!(rendered.contains("skills show: path=.tau/skills"));
    assert!(rendered.contains("name=checklist"));
    assert!(rendered.contains("file=checklist.md"));
    assert!(rendered.contains("content_bytes=17"));
    assert!(rendered.contains("---\nline one\nline two"));
}

#[test]
fn unit_parse_skills_search_args_defaults_and_supports_optional_limit() {
    assert_eq!(
        parse_skills_search_args("checklist").expect("parse default"),
        ("checklist".to_string(), 20)
    );
    assert_eq!(
        parse_skills_search_args("checklist 5").expect("parse explicit"),
        ("checklist".to_string(), 5)
    );
    assert_eq!(
        parse_skills_search_args("secure review 7").expect("parse multiword query"),
        ("secure review".to_string(), 7)
    );
}

#[test]
fn regression_parse_skills_search_args_rejects_missing_query_and_zero_limit() {
    let missing_query = parse_skills_search_args("").expect_err("empty query must fail");
    assert!(missing_query.to_string().contains("query is required"));

    let zero_limit = parse_skills_search_args("checklist 0").expect_err("zero limit must fail");
    assert!(zero_limit
        .to_string()
        .contains("max_results must be greater than zero"));
}

#[test]
fn unit_parse_skills_lock_diff_args_supports_defaults_path_override_and_json() {
    let default_lock = PathBuf::from(".tau/skills/skills.lock.json");
    assert_eq!(
        parse_skills_lock_diff_args("", &default_lock).expect("default parse"),
        (default_lock.clone(), false)
    );
    assert_eq!(
        parse_skills_lock_diff_args("--json", &default_lock).expect("json parse"),
        (default_lock.clone(), true)
    );
    assert_eq!(
        parse_skills_lock_diff_args("/tmp/custom.lock.json --json", &default_lock)
            .expect("path + json parse"),
        (PathBuf::from("/tmp/custom.lock.json"), true)
    );
}

#[test]
fn regression_parse_skills_lock_diff_args_rejects_extra_positional_args() {
    let default_lock = PathBuf::from(".tau/skills/skills.lock.json");
    let error = parse_skills_lock_diff_args("one two", &default_lock).expect_err("must fail");
    assert!(error
        .to_string()
        .contains("usage: /skills-lock-diff [lockfile_path] [--json]"));
}

#[test]
fn unit_parse_skills_prune_args_defaults_and_supports_mode_flags() {
    let default_lock = PathBuf::from(".tau/skills/skills.lock.json");
    assert_eq!(
        parse_skills_prune_args("", &default_lock).expect("default parse"),
        (default_lock.clone(), SkillsPruneMode::DryRun)
    );
    assert_eq!(
        parse_skills_prune_args("--apply", &default_lock).expect("apply parse"),
        (default_lock.clone(), SkillsPruneMode::Apply)
    );
    assert_eq!(
        parse_skills_prune_args("/tmp/custom.lock.json --dry-run", &default_lock)
            .expect("path + dry-run parse"),
        (
            PathBuf::from("/tmp/custom.lock.json"),
            SkillsPruneMode::DryRun
        )
    );
}

#[test]
fn regression_parse_skills_prune_args_rejects_conflicts_and_extra_positionals() {
    let default_lock = PathBuf::from(".tau/skills/skills.lock.json");

    let conflict = parse_skills_prune_args("--apply --dry-run", &default_lock)
        .expect_err("conflicting flags should fail");
    assert!(conflict.to_string().contains(SKILLS_PRUNE_USAGE));

    let extra = parse_skills_prune_args("one two", &default_lock)
        .expect_err("extra positional args should fail");
    assert!(extra.to_string().contains(SKILLS_PRUNE_USAGE));
}

#[test]
fn unit_validate_skills_prune_file_name_rejects_unsafe_paths() {
    validate_skills_prune_file_name("checklist.md").expect("simple markdown name should pass");
    assert!(validate_skills_prune_file_name("../checklist.md").is_err());
    assert!(validate_skills_prune_file_name("nested/checklist.md").is_err());
    assert!(validate_skills_prune_file_name(r"nested\checklist.md").is_err());
}

#[test]
fn unit_derive_skills_prune_candidates_filters_tracked_and_sorts() {
    let skills_dir = Path::new(".tau/skills");
    let catalog = vec![
        crate::skills::Skill {
            name: "zeta".to_string(),
            content: "zeta".to_string(),
            path: PathBuf::from(".tau/skills/zeta.md"),
        },
        crate::skills::Skill {
            name: "alpha".to_string(),
            content: "alpha".to_string(),
            path: PathBuf::from(".tau/skills/alpha.md"),
        },
        crate::skills::Skill {
            name: "beta".to_string(),
            content: "beta".to_string(),
            path: PathBuf::from(".tau/skills/beta.md"),
        },
    ];
    let tracked = HashSet::from([String::from("alpha.md")]);
    let candidates =
        derive_skills_prune_candidates(skills_dir, &catalog, &tracked).expect("derive candidates");
    let files = candidates
        .iter()
        .map(|candidate| candidate.file.as_str())
        .collect::<Vec<_>>();
    assert_eq!(files, vec!["beta.md", "zeta.md"]);
}

#[test]
fn regression_resolve_prunable_skill_file_name_rejects_nested_paths() {
    let skills_dir = Path::new(".tau/skills");
    let error = resolve_prunable_skill_file_name(skills_dir, Path::new(".tau/skills/nested/a.md"))
        .expect_err("nested path should fail");
    assert!(error.to_string().contains("nested paths are not allowed"));
}

#[test]
fn unit_parse_skills_trust_mutation_args_supports_configured_and_explicit_paths() {
    let configured = PathBuf::from("/tmp/trust-roots.json");
    assert_eq!(
        parse_skills_trust_mutation_args(
            "root=YQ==",
            Some(configured.as_path()),
            SKILLS_TRUST_ADD_USAGE
        )
        .expect("configured path should be used"),
        ("root=YQ==".to_string(), configured)
    );

    assert_eq!(
        parse_skills_trust_mutation_args(
            "root=YQ== /tmp/override.json",
            Some(Path::new("/tmp/default.json")),
            SKILLS_TRUST_ADD_USAGE
        )
        .expect("explicit path should override configured path"),
        ("root=YQ==".to_string(), PathBuf::from("/tmp/override.json"))
    );
}

#[test]
fn regression_parse_skills_trust_mutation_args_requires_path_without_configuration() {
    let missing = parse_skills_trust_mutation_args("root=YQ==", None, SKILLS_TRUST_ADD_USAGE)
        .expect_err("command should fail without configured/default path");
    assert!(missing.to_string().contains(SKILLS_TRUST_ADD_USAGE));

    let extra = parse_skills_trust_mutation_args(
        "one two three",
        Some(Path::new("/tmp/default.json")),
        SKILLS_TRUST_ADD_USAGE,
    )
    .expect_err("extra positional args should fail");
    assert!(extra.to_string().contains(SKILLS_TRUST_ADD_USAGE));
}

#[test]
fn unit_parse_skills_verify_args_supports_defaults_overrides_and_json() {
    let default_lock = Path::new("/tmp/default.lock.json");
    let default_trust = Path::new("/tmp/default-trust.json");

    let parsed =
        parse_skills_verify_args("", default_lock, Some(default_trust)).expect("parse defaults");
    assert_eq!(parsed.lock_path, PathBuf::from(default_lock));
    assert_eq!(parsed.trust_root_path, Some(PathBuf::from(default_trust)));
    assert!(!parsed.json_output);

    let parsed = parse_skills_verify_args(
        "/tmp/custom.lock.json /tmp/custom-trust.json --json",
        default_lock,
        Some(default_trust),
    )
    .expect("parse explicit args");
    assert_eq!(parsed.lock_path, PathBuf::from("/tmp/custom.lock.json"));
    assert_eq!(
        parsed.trust_root_path,
        Some(PathBuf::from("/tmp/custom-trust.json"))
    );
    assert!(parsed.json_output);
}

#[test]
fn regression_parse_skills_verify_args_rejects_unexpected_extra_positionals() {
    let error = parse_skills_verify_args(
        "a b c",
        Path::new("/tmp/default.lock.json"),
        Some(Path::new("/tmp/default-trust.json")),
    )
    .expect_err("unexpected positional arguments should fail");
    assert!(error.to_string().contains(SKILLS_VERIFY_USAGE));
}

#[test]
fn unit_parse_skills_trust_list_args_supports_configured_and_explicit_paths() {
    let configured = PathBuf::from("/tmp/trust-roots.json");
    assert_eq!(
        parse_skills_trust_list_args("", Some(configured.as_path()))
            .expect("configured path should be used"),
        configured
    );

    assert_eq!(
        parse_skills_trust_list_args("/tmp/override.json", Some(Path::new("/tmp/default.json")))
            .expect("explicit path should override configured path"),
        PathBuf::from("/tmp/override.json")
    );
}

#[test]
fn regression_parse_skills_trust_list_args_requires_path_without_configuration() {
    let missing = parse_skills_trust_list_args("", None)
        .expect_err("command should fail without configured/default path");
    assert!(missing.to_string().contains(SKILLS_TRUST_LIST_USAGE));

    let extra = parse_skills_trust_list_args("one two", Some(Path::new("/tmp/default.json")))
        .expect_err("extra positional args should fail");
    assert!(extra.to_string().contains(SKILLS_TRUST_LIST_USAGE));
}

#[test]
fn unit_trust_record_status_reports_active_revoked_and_expired() {
    let active = TrustedRootRecord {
        id: "active".to_string(),
        public_key: "YQ==".to_string(),
        revoked: false,
        expires_unix: None,
        rotated_from: None,
    };
    let revoked = TrustedRootRecord {
        id: "revoked".to_string(),
        public_key: "Yg==".to_string(),
        revoked: true,
        expires_unix: None,
        rotated_from: None,
    };
    let expired = TrustedRootRecord {
        id: "expired".to_string(),
        public_key: "Yw==".to_string(),
        revoked: false,
        expires_unix: Some(1),
        rotated_from: None,
    };

    assert_eq!(trust_record_status(&active, 10), "active");
    assert_eq!(trust_record_status(&revoked, 10), "revoked");
    assert_eq!(trust_record_status(&expired, 10), "expired");
}

#[test]
fn unit_render_skills_trust_list_handles_empty_records() {
    let rendered = render_skills_trust_list(Path::new(".tau/trust-roots.json"), &[]);
    assert!(rendered.contains("skills trust list: path=.tau/trust-roots.json count=0"));
    assert!(rendered.contains("roots: none"));
}

#[test]
fn unit_render_skills_lock_diff_helpers_include_expected_prefixes() {
    let report = crate::skills::SkillsSyncReport {
        expected_entries: 1,
        actual_entries: 1,
        ..crate::skills::SkillsSyncReport::default()
    };
    let in_sync = render_skills_lock_diff_in_sync(Path::new("skills.lock.json"), &report);
    assert!(in_sync.contains("skills lock diff: in-sync"));

    let drift = render_skills_lock_diff_drift(Path::new("skills.lock.json"), &report);
    assert!(drift.contains("skills lock diff: drift"));
}

#[test]
fn unit_render_skills_search_handles_empty_results() {
    let rendered = render_skills_search(Path::new(".tau/skills"), "missing", 10, &[], 0);
    assert!(rendered.contains("skills search: path=.tau/skills"));
    assert!(rendered.contains("query=\"missing\""));
    assert!(rendered.contains("matched=0"));
    assert!(rendered.contains("shown=0"));
    assert!(rendered.contains("skills: none"));
}

#[test]
fn functional_execute_skills_list_command_reports_sorted_inventory() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("zeta.md"), "zeta").expect("write zeta");
    std::fs::write(skills_dir.join("alpha.md"), "alpha").expect("write alpha");
    std::fs::write(skills_dir.join("ignored.txt"), "ignored").expect("write ignored");

    let output = execute_skills_list_command(&skills_dir);
    assert!(output.contains("count=2"));
    let alpha_index = output
        .find("skill: name=alpha file=alpha.md")
        .expect("alpha");
    let zeta_index = output.find("skill: name=zeta file=zeta.md").expect("zeta");
    assert!(alpha_index < zeta_index);
}

#[test]
fn regression_execute_skills_list_command_reports_errors_without_panicking() {
    let temp = tempdir().expect("tempdir");
    let not_a_dir = temp.path().join("skills.md");
    std::fs::write(&not_a_dir, "not a directory").expect("write file");

    let output = execute_skills_list_command(&not_a_dir);
    assert!(output.contains("skills list error: path="));
    assert!(output.contains("is not a directory"));
}

#[test]
fn functional_execute_skills_search_command_ranks_name_hits_before_content_hits() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("checklist.md"), "Always run tests").expect("write checklist");
    std::fs::write(skills_dir.join("quality.md"), "Use checklist for review")
        .expect("write quality");

    let output = execute_skills_search_command(&skills_dir, "checklist");
    assert!(output.contains("skills search: path="));
    assert!(output.contains("matched=2"));
    let checklist_index = output
        .find("skill: name=checklist file=checklist.md match=name")
        .expect("checklist row");
    let quality_index = output
        .find("skill: name=quality file=quality.md match=content")
        .expect("quality row");
    assert!(checklist_index < quality_index);
}

#[test]
fn regression_execute_skills_search_command_reports_invalid_args_without_panicking() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("checklist.md"), "Always run tests").expect("write skill");

    let output = execute_skills_search_command(&skills_dir, "checklist 0");
    assert!(output.contains("skills search error: path="));
    assert!(output.contains("max_results must be greater than zero"));
}

#[test]
fn functional_execute_skills_lock_diff_command_supports_human_and_json_output() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

    let lock_path = default_skills_lock_path(&skills_dir);
    let sha = format!("{:x}", Sha256::digest("deterministic body".as_bytes()));
    let lockfile = serde_json::json!({
        "schema_version": 1,
        "entries": [{
            "name": "focus",
            "file": "focus.md",
            "sha256": sha,
            "source": {
                "kind": "unknown"
            }
        }]
    });
    std::fs::write(&lock_path, format!("{lockfile}\n")).expect("write lock");

    let human = execute_skills_lock_diff_command(&skills_dir, &lock_path, "");
    assert!(human.contains("skills lock diff: in-sync"));
    assert!(human.contains("expected_entries=1"));

    let json_output = execute_skills_lock_diff_command(&skills_dir, &lock_path, "--json");
    let payload: serde_json::Value = serde_json::from_str(&json_output).expect("parse json output");
    assert_eq!(payload["status"], "in_sync");
    assert_eq!(payload["in_sync"], true);
    assert_eq!(payload["expected_entries"], 1);
    assert_eq!(payload["actual_entries"], 1);
}

#[test]
fn regression_execute_skills_lock_diff_command_reports_missing_lockfile_errors() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

    let missing_lock_path = temp.path().join("missing.lock.json");
    let output = execute_skills_lock_diff_command(
        &skills_dir,
        &default_skills_lock_path(&skills_dir),
        missing_lock_path.to_str().expect("utf8 path"),
    );
    assert!(output.contains("skills lock diff error: path="));
    assert!(output.contains("failed to read skills lockfile"));
}

#[test]
fn functional_execute_skills_prune_command_supports_dry_run_and_apply() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("tracked.md"), "tracked body").expect("write tracked");
    std::fs::write(skills_dir.join("stale.md"), "stale body").expect("write stale");

    let lock_path = default_skills_lock_path(&skills_dir);
    let tracked_sha = format!("{:x}", Sha256::digest("tracked body".as_bytes()));
    let lockfile = serde_json::json!({
        "schema_version": 1,
        "entries": [{
            "name": "tracked",
            "file": "tracked.md",
            "sha256": tracked_sha,
            "source": {
                "kind": "unknown"
            }
        }]
    });
    std::fs::write(&lock_path, format!("{lockfile}\n")).expect("write lockfile");

    let dry_run = execute_skills_prune_command(&skills_dir, &lock_path, "");
    assert!(dry_run.contains("skills prune: mode=dry-run"));
    assert!(dry_run.contains("prune: file=stale.md action=would_delete"));
    assert!(skills_dir.join("stale.md").exists());

    let apply = execute_skills_prune_command(&skills_dir, &lock_path, "--apply");
    assert!(apply.contains("skills prune: mode=apply"));
    assert!(apply.contains("prune: file=stale.md action=delete"));
    assert!(apply.contains("prune: file=stale.md status=deleted"));
    assert!(apply.contains("skills prune result: mode=apply deleted=1 failed=0"));
    assert!(skills_dir.join("tracked.md").exists());
    assert!(!skills_dir.join("stale.md").exists());
}

#[test]
fn regression_execute_skills_prune_command_reports_missing_lockfile_errors() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("stale.md"), "stale body").expect("write stale");

    let missing_lock_path = temp.path().join("missing.lock.json");
    let output = execute_skills_prune_command(
        &skills_dir,
        &default_skills_lock_path(&skills_dir),
        missing_lock_path.to_str().expect("utf8 path"),
    );
    assert!(output.contains("skills prune error: path="));
    assert!(output.contains("failed to read skills lockfile"));
}

#[test]
fn regression_execute_skills_prune_command_rejects_unsafe_lockfile_entries() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("stale.md"), "stale body").expect("write stale");

    let lock_path = default_skills_lock_path(&skills_dir);
    let lockfile = serde_json::json!({
        "schema_version": 1,
        "entries": [{
            "name": "escape",
            "file": "../escape.md",
            "sha256": "abc123",
            "source": {
                "kind": "unknown"
            }
        }]
    });
    std::fs::write(&lock_path, format!("{lockfile}\n")).expect("write lockfile");

    let output = execute_skills_prune_command(&skills_dir, &lock_path, "");
    assert!(output.contains("skills prune error: path="));
    assert!(output.contains("unsafe lockfile entry '../escape.md'"));
}

#[test]
fn functional_execute_skills_trust_list_command_supports_default_and_explicit_paths() {
    let temp = tempdir().expect("tempdir");
    let default_trust_path = temp.path().join("trust-roots.json");
    let explicit_trust_path = temp.path().join("explicit-trust-roots.json");
    let payload = serde_json::json!({
        "roots": [
            {
                "id": "zeta",
                "public_key": "eg==",
                "revoked": false,
                "expires_unix": 1,
                "rotated_from": null
            },
            {
                "id": "alpha",
                "public_key": "YQ==",
                "revoked": false,
                "expires_unix": null,
                "rotated_from": null
            },
            {
                "id": "beta",
                "public_key": "Yg==",
                "revoked": true,
                "expires_unix": null,
                "rotated_from": "alpha"
            }
        ]
    });
    std::fs::write(&default_trust_path, format!("{payload}\n")).expect("write default trust");
    std::fs::write(&explicit_trust_path, format!("{payload}\n")).expect("write explicit trust");

    let default_output = execute_skills_trust_list_command(Some(default_trust_path.as_path()), "");
    assert!(default_output.contains("skills trust list: path="));
    assert!(default_output.contains("count=3"));
    let alpha_index = default_output.find("root: id=alpha").expect("alpha row");
    let beta_index = default_output.find("root: id=beta").expect("beta row");
    let zeta_index = default_output.find("root: id=zeta").expect("zeta row");
    assert!(alpha_index < beta_index);
    assert!(beta_index < zeta_index);
    assert!(default_output.contains(
        "root: id=beta revoked=true expires_unix=none rotated_from=alpha status=revoked"
    ));
    assert!(default_output
        .contains("root: id=zeta revoked=false expires_unix=1 rotated_from=none status=expired"));

    let explicit_output =
        execute_skills_trust_list_command(None, explicit_trust_path.to_str().expect("utf8 path"));
    assert!(explicit_output.contains("skills trust list: path="));
    assert!(explicit_output.contains("count=3"));
}

#[test]
fn functional_render_skills_verify_report_includes_summary_sync_and_entries() {
    let report = SkillsVerifyReport {
        lock_path: "/tmp/skills.lock.json".to_string(),
        trust_root_path: Some("/tmp/trust-roots.json".to_string()),
        expected_entries: 2,
        actual_entries: 2,
        missing: vec![],
        extra: vec![],
        changed: vec![],
        metadata_mismatch: vec![],
        trust: Some(SkillsVerifyTrustSummary {
            total: 1,
            active: 1,
            revoked: 0,
            expired: 0,
        }),
        summary: SkillsVerifySummary {
            entries: 2,
            pass: 2,
            warn: 0,
            fail: 0,
            status: SkillsVerifyStatus::Pass,
        },
        entries: vec![SkillsVerifyEntry {
            file: "focus.md".to_string(),
            name: "focus".to_string(),
            status: SkillsVerifyStatus::Pass,
            checks: vec![
                "sync=ok".to_string(),
                "signature=trusted key=root".to_string(),
            ],
        }],
    };

    let rendered = render_skills_verify_report(&report);
    assert!(rendered.contains(
            "skills verify: status=pass lock_path=/tmp/skills.lock.json trust_root_path=/tmp/trust-roots.json"
        ));
    assert!(rendered.contains(
            "sync: expected_entries=2 actual_entries=2 missing=none extra=none changed=none metadata=none"
        ));
    assert!(rendered.contains("trust: total=1 active=1 revoked=0 expired=0"));
    assert!(rendered.contains(
        "entry: file=focus.md name=focus status=pass checks=sync=ok;signature=trusted key=root"
    ));
}

#[test]
fn integration_execute_skills_verify_command_reports_pass_and_json_modes() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

    let lock_path = default_skills_lock_path(&skills_dir);
    let trust_path = temp.path().join("trust-roots.json");
    let skill_sha = format!("{:x}", Sha256::digest("deterministic body".as_bytes()));
    let signature = "c2ln";
    let signature_sha = format!("{:x}", Sha256::digest(signature.as_bytes()));
    let lockfile = serde_json::json!({
        "schema_version": 1,
        "entries": [{
            "name": "focus",
            "file": "focus.md",
            "sha256": skill_sha,
            "source": {
                "kind": "remote",
                "url": "https://example.com/focus.md",
                "expected_sha256": skill_sha,
                "signing_key_id": "root",
                "signature": signature,
                "signer_public_key": "YQ==",
                "signature_sha256": signature_sha
            }
        }]
    });
    std::fs::write(&lock_path, format!("{lockfile}\n")).expect("write lock");
    let trust = serde_json::json!({
        "roots": [{
            "id": "root",
            "public_key": "YQ==",
            "revoked": false,
            "expires_unix": null,
            "rotated_from": null
        }]
    });
    std::fs::write(&trust_path, format!("{trust}\n")).expect("write trust");

    let output =
        execute_skills_verify_command(&skills_dir, &lock_path, Some(trust_path.as_path()), "");
    assert!(output.contains("skills verify: status=pass"));
    assert!(output.contains("sync: expected_entries=1 actual_entries=1"));
    assert!(output.contains("entry: file=focus.md name=focus status=pass"));
    assert!(output.contains("signature=trusted key=root"));

    let json_output = execute_skills_verify_command(
        &skills_dir,
        &lock_path,
        Some(trust_path.as_path()),
        "--json",
    );
    let payload: serde_json::Value = serde_json::from_str(&json_output).expect("parse verify json");
    assert_eq!(payload["summary"]["status"], "pass");
    assert_eq!(payload["summary"]["fail"], 0);
    assert_eq!(payload["entries"][0]["status"], "pass");
}

#[test]
fn regression_execute_skills_verify_command_reports_untrusted_signing_key() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

    let lock_path = default_skills_lock_path(&skills_dir);
    let trust_path = temp.path().join("trust-roots.json");
    let skill_sha = format!("{:x}", Sha256::digest("deterministic body".as_bytes()));
    let signature = "c2ln";
    let signature_sha = format!("{:x}", Sha256::digest(signature.as_bytes()));
    let lockfile = serde_json::json!({
        "schema_version": 1,
        "entries": [{
            "name": "focus",
            "file": "focus.md",
            "sha256": skill_sha,
            "source": {
                "kind": "remote",
                "url": "https://example.com/focus.md",
                "expected_sha256": skill_sha,
                "signing_key_id": "unknown",
                "signature": signature,
                "signer_public_key": "YQ==",
                "signature_sha256": signature_sha
            }
        }]
    });
    std::fs::write(&lock_path, format!("{lockfile}\n")).expect("write lock");
    let trust = serde_json::json!({
        "roots": [{
            "id": "root",
            "public_key": "YQ==",
            "revoked": false,
            "expires_unix": null,
            "rotated_from": null
        }]
    });
    std::fs::write(&trust_path, format!("{trust}\n")).expect("write trust");

    let output =
        execute_skills_verify_command(&skills_dir, &lock_path, Some(trust_path.as_path()), "");
    assert!(output.contains("skills verify: status=fail"));
    assert!(output.contains("signature=untrusted key=unknown"));
}

#[test]
fn regression_execute_skills_verify_command_reports_missing_lockfile() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    let lock_path = temp.path().join("missing.lock.json");

    let output = execute_skills_verify_command(&skills_dir, &lock_path, None, "");
    assert!(output.contains("skills verify error: path="));
    assert!(output.contains("failed to read skills lockfile"));
}

#[test]
fn functional_execute_skills_trust_mutation_commands_round_trip_updates_store() {
    let temp = tempdir().expect("tempdir");
    let trust_path = temp.path().join("trust-roots.json");
    let payload = serde_json::json!({
        "roots": [
            {
                "id": "old",
                "public_key": "YQ==",
                "revoked": false,
                "expires_unix": null,
                "rotated_from": null
            }
        ]
    });
    std::fs::write(&trust_path, format!("{payload}\n")).expect("write trust file");

    let add_output = execute_skills_trust_add_command(Some(trust_path.as_path()), "extra=Yg==");
    assert!(add_output.contains("skills trust add: path="));
    assert!(add_output.contains("id=extra"));
    assert!(add_output.contains("added=1"));

    let revoke_output = execute_skills_trust_revoke_command(Some(trust_path.as_path()), "extra");
    assert!(revoke_output.contains("skills trust revoke: path="));
    assert!(revoke_output.contains("id=extra"));
    assert!(revoke_output.contains("revoked=1"));

    let rotate_output =
        execute_skills_trust_rotate_command(Some(trust_path.as_path()), "old:new=Yw==");
    assert!(rotate_output.contains("skills trust rotate: path="));
    assert!(rotate_output.contains("old_id=old"));
    assert!(rotate_output.contains("new_id=new"));
    assert!(rotate_output.contains("rotated=1"));

    let list_output = execute_skills_trust_list_command(Some(trust_path.as_path()), "");
    assert!(list_output.contains("skills trust list: path="));
    assert!(list_output.contains("root: id=old"));
    assert!(list_output.contains("status=revoked"));
    assert!(list_output.contains("root: id=new"));
    assert!(list_output.contains("rotated_from=old status=active"));
    assert!(list_output.contains("root: id=extra"));
    assert!(list_output.contains("status=revoked"));
}

#[test]
fn regression_execute_skills_trust_add_command_requires_path_without_configuration() {
    let output = execute_skills_trust_add_command(None, "root=YQ==");
    assert!(output.contains("skills trust add error: path=none"));
    assert!(output.contains(SKILLS_TRUST_ADD_USAGE));
}

#[test]
fn regression_execute_skills_trust_revoke_command_reports_unknown_id() {
    let temp = tempdir().expect("tempdir");
    let trust_path = temp.path().join("trust-roots.json");
    std::fs::write(&trust_path, "[]\n").expect("write trust file");

    let output = execute_skills_trust_revoke_command(Some(trust_path.as_path()), "missing");
    assert!(output.contains("skills trust revoke error: path="));
    assert!(output.contains("cannot revoke unknown trust key id 'missing'"));
}

#[test]
fn regression_execute_skills_trust_rotate_command_reports_invalid_spec() {
    let temp = tempdir().expect("tempdir");
    let trust_path = temp.path().join("trust-roots.json");
    std::fs::write(&trust_path, "[]\n").expect("write trust file");

    let output = execute_skills_trust_rotate_command(Some(trust_path.as_path()), "bad-shape");
    assert!(output.contains("skills trust rotate error: path="));
    assert!(output.contains("expected old_id:new_id=base64_key"));
}

#[test]
fn regression_execute_skills_trust_list_command_reports_malformed_json() {
    let temp = tempdir().expect("tempdir");
    let trust_path = temp.path().join("trust-roots.json");
    std::fs::write(&trust_path, "{not-json").expect("write malformed trust file");

    let output = execute_skills_trust_list_command(None, trust_path.to_str().expect("utf8 path"));
    assert!(output.contains("skills trust list error: path="));
    assert!(output.contains("failed to parse trusted root file"));
}

#[test]
fn functional_execute_skills_show_command_displays_selected_skill() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("checklist.md"), "Always run tests").expect("write skill");

    let output = execute_skills_show_command(&skills_dir, "checklist");
    assert!(output.contains("skills show: path="));
    assert!(output.contains("name=checklist"));
    assert!(output.contains("file=checklist.md"));
    assert!(output.contains("Always run tests"));
}

#[test]
fn regression_execute_skills_show_command_reports_unknown_skill_without_panicking() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("known.md"), "Known").expect("write skill");

    let output = execute_skills_show_command(&skills_dir, "missing");
    assert!(output.contains("skills show error: path="));
    assert!(output.contains("error=unknown skill 'missing'"));
}

#[test]
fn functional_execute_skills_lock_write_command_writes_default_lockfile() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

    let lock_path = default_skills_lock_path(&skills_dir);
    let output = execute_skills_lock_write_command(&skills_dir, &lock_path, "");
    assert!(output.contains("skills lock write: path="));
    assert!(output.contains("entries=1"));

    let lock_raw = std::fs::read_to_string(lock_path).expect("read lockfile");
    assert!(lock_raw.contains("\"file\": \"focus.md\""));
}

#[test]
fn regression_execute_skills_lock_write_command_reports_write_errors_without_panicking() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

    let blocking_path = temp.path().join("lock-as-dir");
    std::fs::create_dir_all(&blocking_path).expect("create blocking dir");

    let output = execute_skills_lock_write_command(
        &skills_dir,
        &default_skills_lock_path(&skills_dir),
        blocking_path.to_str().expect("utf8 path"),
    );
    assert!(output.contains("skills lock write error: path="));
    assert!(
        output.contains("failed to read skills lockfile")
            || output.contains("failed to write skills lockfile")
    );
}

#[test]
fn functional_execute_skills_sync_command_reports_in_sync_for_default_lock_path() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

    let lock_path = default_skills_lock_path(&skills_dir);
    let sha = format!("{:x}", Sha256::digest("deterministic body".as_bytes()));
    let lockfile = serde_json::json!({
        "schema_version": 1,
        "entries": [{
            "name": "focus",
            "file": "focus.md",
            "sha256": sha,
            "source": {
                "kind": "unknown"
            }
        }]
    });
    std::fs::write(&lock_path, format!("{lockfile}\n")).expect("write lockfile");

    let output = execute_skills_sync_command(&skills_dir, &lock_path, "");
    assert!(output.contains("skills sync: in-sync"));
    assert!(output.contains("expected_entries=1"));
    assert!(output.contains("actual_entries=1"));
}

#[test]
fn regression_execute_skills_sync_command_reports_lockfile_errors_without_panicking() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

    let missing_lock_path = temp.path().join("missing.lock.json");
    let output = execute_skills_sync_command(
        &skills_dir,
        &default_skills_lock_path(&skills_dir),
        missing_lock_path.to_str().expect("utf8 path"),
    );

    assert!(output.contains("skills sync error: path="));
    assert!(output.contains("failed to read skills lockfile"));
}

#[test]
fn functional_help_command_returns_continue_action() {
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let mut runtime = None;
    let tool_policy_json = test_tool_policy_json();

    let action = handle_command("/help branch", &mut agent, &mut runtime, &tool_policy_json)
        .expect("help should succeed");
    assert_eq!(action, CommandAction::Continue);
}

#[test]
fn functional_audit_summary_command_without_path_returns_continue_action() {
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let mut runtime = None;
    let tool_policy_json = test_tool_policy_json();

    let action = handle_command(
        "/audit-summary",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
    )
    .expect("audit summary usage should not fail");
    assert_eq!(action, CommandAction::Continue);
}

#[test]
fn integration_skills_sync_command_preserves_session_runtime_on_drift() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("focus.md"), "actual body").expect("write skill");
    let lock_path = default_skills_lock_path(&skills_dir);
    let lockfile = serde_json::json!({
        "schema_version": 1,
        "entries": [{
            "name": "focus",
            "file": "focus.md",
            "sha256": "deadbeef",
            "source": {
                "kind": "unknown"
            }
        }]
    });
    std::fs::write(&lock_path, format!("{lockfile}\n")).expect("write lock");

    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let head = store
        .append_messages(Some(root), &[tau_ai::Message::user("hello")])
        .expect("append user")
        .expect("head id");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = store.lineage_messages(Some(head)).expect("lineage");
    agent.replace_messages(lineage.clone());

    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

    let action = handle_command_with_session_import_mode(
        "/skills-sync",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &test_auth_command_config(),
        &ModelCatalog::built_in(),
        &[],
    )
    .expect("skills sync command should continue");
    assert_eq!(action, CommandAction::Continue);

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.active_head, Some(head));
    assert_eq!(runtime.store.entries().len(), 2);
    assert_eq!(agent.messages().len(), lineage.len());
}

#[test]
fn integration_skills_lock_write_command_preserves_session_runtime_on_error() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("focus.md"), "actual body").expect("write skill");
    let lock_path = default_skills_lock_path(&skills_dir);
    let blocking_path = temp.path().join("lock-as-dir");
    std::fs::create_dir_all(&blocking_path).expect("blocking dir");

    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let head = store
        .append_messages(Some(root), &[tau_ai::Message::user("hello")])
        .expect("append user")
        .expect("head id");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = store.lineage_messages(Some(head)).expect("lineage");
    agent.replace_messages(lineage.clone());

    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

    let action = handle_command_with_session_import_mode(
        &format!("/skills-lock-write {}", blocking_path.display()),
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &test_auth_command_config(),
        &ModelCatalog::built_in(),
        &[],
    )
    .expect("skills lock write command should continue");
    assert_eq!(action, CommandAction::Continue);

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.active_head, Some(head));
    assert_eq!(runtime.store.entries().len(), 2);
    assert_eq!(agent.messages().len(), lineage.len());
}

#[test]
fn integration_skills_list_command_preserves_session_runtime() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");
    std::fs::write(skills_dir.join("beta.md"), "beta body").expect("write beta");
    let lock_path = default_skills_lock_path(&skills_dir);

    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let head = store
        .append_messages(Some(root), &[tau_ai::Message::user("hello")])
        .expect("append user")
        .expect("head id");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = store.lineage_messages(Some(head)).expect("lineage");
    agent.replace_messages(lineage.clone());

    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

    let action = handle_command_with_session_import_mode(
        "/skills-list",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &test_auth_command_config(),
        &ModelCatalog::built_in(),
        &[],
    )
    .expect("skills list command should continue");
    assert_eq!(action, CommandAction::Continue);

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.active_head, Some(head));
    assert_eq!(runtime.store.entries().len(), 2);
    assert_eq!(agent.messages().len(), lineage.len());
}

#[test]
fn integration_skills_show_command_preserves_session_runtime_on_unknown_skill() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");
    let lock_path = default_skills_lock_path(&skills_dir);

    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let head = store
        .append_messages(Some(root), &[tau_ai::Message::user("hello")])
        .expect("append user")
        .expect("head id");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = store.lineage_messages(Some(head)).expect("lineage");
    agent.replace_messages(lineage.clone());

    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

    let action = handle_command_with_session_import_mode(
        "/skills-show missing",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &test_auth_command_config(),
        &ModelCatalog::built_in(),
        &[],
    )
    .expect("skills show command should continue");
    assert_eq!(action, CommandAction::Continue);

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.active_head, Some(head));
    assert_eq!(runtime.store.entries().len(), 2);
    assert_eq!(agent.messages().len(), lineage.len());
}

#[test]
fn integration_skills_search_command_preserves_session_runtime_on_invalid_args() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");
    let lock_path = default_skills_lock_path(&skills_dir);

    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let head = store
        .append_messages(Some(root), &[tau_ai::Message::user("hello")])
        .expect("append user")
        .expect("head id");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = store.lineage_messages(Some(head)).expect("lineage");
    agent.replace_messages(lineage.clone());

    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

    let action = handle_command_with_session_import_mode(
        "/skills-search alpha 0",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &test_auth_command_config(),
        &ModelCatalog::built_in(),
        &[],
    )
    .expect("skills search command should continue");
    assert_eq!(action, CommandAction::Continue);

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.active_head, Some(head));
    assert_eq!(runtime.store.entries().len(), 2);
    assert_eq!(agent.messages().len(), lineage.len());
}

#[test]
fn integration_skills_lock_diff_command_preserves_session_runtime_on_error() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");
    let lock_path = default_skills_lock_path(&skills_dir);

    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let head = store
        .append_messages(Some(root), &[tau_ai::Message::user("hello")])
        .expect("append user")
        .expect("head id");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = store.lineage_messages(Some(head)).expect("lineage");
    agent.replace_messages(lineage.clone());

    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

    let action = handle_command_with_session_import_mode(
        "/skills-lock-diff /tmp/missing.lock.json",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &test_auth_command_config(),
        &ModelCatalog::built_in(),
        &[],
    )
    .expect("skills lock diff command should continue");
    assert_eq!(action, CommandAction::Continue);

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.active_head, Some(head));
    assert_eq!(runtime.store.entries().len(), 2);
    assert_eq!(agent.messages().len(), lineage.len());
}

#[test]
fn integration_skills_verify_command_preserves_session_runtime_on_error() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");
    let lock_path = default_skills_lock_path(&skills_dir);

    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let head = store
        .append_messages(Some(root), &[tau_ai::Message::user("hello")])
        .expect("append user")
        .expect("head id");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = store.lineage_messages(Some(head)).expect("lineage");
    agent.replace_messages(lineage.clone());

    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

    let action = handle_command_with_session_import_mode(
        "/skills-verify /tmp/missing.lock.json",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &test_auth_command_config(),
        &ModelCatalog::built_in(),
        &[],
    )
    .expect("skills verify command should continue");
    assert_eq!(action, CommandAction::Continue);

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.active_head, Some(head));
    assert_eq!(runtime.store.entries().len(), 2);
    assert_eq!(agent.messages().len(), lineage.len());
}

#[test]
fn integration_skills_prune_command_preserves_session_runtime_on_error() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");
    let lock_path = default_skills_lock_path(&skills_dir);

    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let head = store
        .append_messages(Some(root), &[tau_ai::Message::user("hello")])
        .expect("append user")
        .expect("head id");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = store.lineage_messages(Some(head)).expect("lineage");
    agent.replace_messages(lineage.clone());

    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

    let action = handle_command_with_session_import_mode(
        "/skills-prune /tmp/missing.lock.json --apply",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &test_auth_command_config(),
        &ModelCatalog::built_in(),
        &[],
    )
    .expect("skills prune command should continue");
    assert_eq!(action, CommandAction::Continue);

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.active_head, Some(head));
    assert_eq!(runtime.store.entries().len(), 2);
    assert_eq!(agent.messages().len(), lineage.len());
}

#[test]
fn integration_skills_trust_list_command_preserves_session_runtime_on_error() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");
    let lock_path = default_skills_lock_path(&skills_dir);
    let trust_path = temp.path().join("trust-roots.json");
    std::fs::write(&trust_path, "{invalid-json").expect("write malformed trust file");

    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let head = store
        .append_messages(Some(root), &[tau_ai::Message::user("hello")])
        .expect("append user")
        .expect("head id");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = store.lineage_messages(Some(head)).expect("lineage");
    agent.replace_messages(lineage.clone());

    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let skills_command_config =
        skills_command_config(&skills_dir, &lock_path, Some(trust_path.as_path()));

    let action = handle_command_with_session_import_mode(
        "/skills-trust-list",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &test_auth_command_config(),
        &ModelCatalog::built_in(),
        &[],
    )
    .expect("skills trust list command should continue");
    assert_eq!(action, CommandAction::Continue);

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.active_head, Some(head));
    assert_eq!(runtime.store.entries().len(), 2);
    assert_eq!(agent.messages().len(), lineage.len());
}

#[test]
fn integration_skills_trust_mutation_commands_update_store_and_preserve_runtime() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");
    let lock_path = default_skills_lock_path(&skills_dir);
    let trust_path = temp.path().join("trust-roots.json");
    std::fs::write(&trust_path, "[]\n").expect("write empty trust file");

    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let head = store
        .append_messages(Some(root), &[tau_ai::Message::user("hello")])
        .expect("append user")
        .expect("head id");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = store.lineage_messages(Some(head)).expect("lineage");
    agent.replace_messages(lineage.clone());

    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let skills_command_config =
        skills_command_config(&skills_dir, &lock_path, Some(trust_path.as_path()));

    let action = handle_command_with_session_import_mode(
        "/skills-trust-add root=YQ==",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &test_auth_command_config(),
        &ModelCatalog::built_in(),
        &[],
    )
    .expect("skills trust add command should continue");
    assert_eq!(action, CommandAction::Continue);

    let action = handle_command_with_session_import_mode(
        "/skills-trust-revoke root",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &test_auth_command_config(),
        &ModelCatalog::built_in(),
        &[],
    )
    .expect("skills trust revoke command should continue");
    assert_eq!(action, CommandAction::Continue);

    let action = handle_command_with_session_import_mode(
        "/skills-trust-rotate root:new=Yg==",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &test_auth_command_config(),
        &ModelCatalog::built_in(),
        &[],
    )
    .expect("skills trust rotate command should continue");
    assert_eq!(action, CommandAction::Continue);

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.active_head, Some(head));
    assert_eq!(runtime.store.entries().len(), 2);
    assert_eq!(agent.messages().len(), lineage.len());

    let records = load_trust_root_records(&trust_path).expect("load trust records");
    let root_record = records
        .iter()
        .find(|record| record.id == "root")
        .expect("root");
    let new_record = records
        .iter()
        .find(|record| record.id == "new")
        .expect("new");
    assert!(root_record.revoked);
    assert!(!new_record.revoked);
    assert_eq!(new_record.rotated_from.as_deref(), Some("root"));
}

#[test]
fn functional_resolve_prompt_input_reads_prompt_file() {
    let temp = tempdir().expect("tempdir");
    let prompt_path = temp.path().join("prompt.txt");
    std::fs::write(&prompt_path, "file prompt\nline two").expect("write prompt");

    let mut cli = test_cli();
    cli.prompt_file = Some(prompt_path);

    let prompt = resolve_prompt_input(&cli).expect("resolve prompt from file");
    assert_eq!(prompt.as_deref(), Some("file prompt\nline two"));
}

#[test]
fn functional_resolve_prompt_input_renders_prompt_template_file() {
    let temp = tempdir().expect("tempdir");
    let template_path = temp.path().join("prompt-template.txt");
    std::fs::write(
        &template_path,
        "Summarize {{module}} with focus on {{focus}}.",
    )
    .expect("write template");

    let mut cli = test_cli();
    cli.prompt_template_file = Some(template_path);
    cli.prompt_template_var = vec![
        "module=src/main.rs".to_string(),
        "focus=error handling".to_string(),
    ];

    let prompt = resolve_prompt_input(&cli).expect("resolve rendered template");
    assert_eq!(
        prompt.as_deref(),
        Some("Summarize src/main.rs with focus on error handling.")
    );
}

#[test]
fn regression_resolve_prompt_input_rejects_empty_prompt_file() {
    let temp = tempdir().expect("tempdir");
    let prompt_path = temp.path().join("prompt.txt");
    std::fs::write(&prompt_path, "   \n\t").expect("write prompt");

    let mut cli = test_cli();
    cli.prompt_file = Some(prompt_path.clone());

    let error = resolve_prompt_input(&cli).expect_err("empty prompt should fail");
    assert!(error
        .to_string()
        .contains(&format!("prompt file {} is empty", prompt_path.display())));
}

#[test]
fn regression_resolve_prompt_input_rejects_template_with_missing_variable() {
    let temp = tempdir().expect("tempdir");
    let template_path = temp.path().join("prompt-template.txt");
    std::fs::write(&template_path, "Review {{path}} and {{goal}}").expect("write template");

    let mut cli = test_cli();
    cli.prompt_template_file = Some(template_path);
    cli.prompt_template_var = vec!["path=src/lib.rs".to_string()];

    let error = resolve_prompt_input(&cli).expect_err("missing template var should fail");
    assert!(error
        .to_string()
        .contains("missing a --prompt-template-var value"));
}

#[test]
fn regression_resolve_prompt_input_rejects_invalid_template_var_spec() {
    let temp = tempdir().expect("tempdir");
    let template_path = temp.path().join("prompt-template.txt");
    std::fs::write(&template_path, "Review {{path}}").expect("write template");

    let mut cli = test_cli();
    cli.prompt_template_file = Some(template_path);
    cli.prompt_template_var = vec!["path".to_string()];

    let error = resolve_prompt_input(&cli).expect_err("invalid template var spec should fail");
    assert!(error.to_string().contains("invalid --prompt-template-var"));
}

#[test]
fn regression_resolve_prompt_input_rejects_unused_template_vars() {
    let temp = tempdir().expect("tempdir");
    let template_path = temp.path().join("prompt-template.txt");
    std::fs::write(&template_path, "Review {{path}}").expect("write template");

    let mut cli = test_cli();
    cli.prompt_template_file = Some(template_path);
    cli.prompt_template_var = vec!["path=src/lib.rs".to_string(), "extra=unused".to_string()];

    let error = resolve_prompt_input(&cli).expect_err("unused template vars should fail");
    assert!(error
        .to_string()
        .contains("unused --prompt-template-var keys"));
}

#[test]
fn functional_resolve_secret_from_cli_or_store_id_reads_integration_secret() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("credentials.json");
    write_test_integration_credential(
        &store_path,
        CredentialStoreEncryptionMode::None,
        None,
        "github-token",
        IntegrationCredentialStoreRecord {
            secret: Some("ghp_store_secret".to_string()),
            revoked: false,
            updated_unix: Some(current_unix_timestamp()),
        },
    );

    let mut cli = test_cli();
    cli.credential_store = store_path;
    let resolved =
        resolve_secret_from_cli_or_store_id(&cli, None, Some("github-token"), "--github-token-id")
            .expect("resolve secret")
            .expect("secret should be present");
    assert_eq!(resolved, "ghp_store_secret");
}

#[test]
fn regression_resolve_secret_from_cli_or_store_id_rejects_revoked_secret() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("credentials.json");
    write_test_integration_credential(
        &store_path,
        CredentialStoreEncryptionMode::None,
        None,
        "slack-app-token",
        IntegrationCredentialStoreRecord {
            secret: Some("xapp_secret".to_string()),
            revoked: true,
            updated_unix: Some(current_unix_timestamp()),
        },
    );

    let mut cli = test_cli();
    cli.credential_store = store_path;
    let error = resolve_secret_from_cli_or_store_id(
        &cli,
        None,
        Some("slack-app-token"),
        "--slack-app-token-id",
    )
    .expect_err("revoked secret should fail");
    assert!(error.to_string().contains("is revoked"));
}

#[test]
fn unit_resolve_secret_from_cli_or_store_id_prefers_direct_secret() {
    let cli = test_cli();
    let resolved = resolve_secret_from_cli_or_store_id(
        &cli,
        Some("direct-token"),
        Some("missing-id"),
        "--github-token-id",
    )
    .expect("resolve direct secret")
    .expect("secret");
    assert_eq!(resolved, "direct-token");
}

#[test]
fn unit_validate_github_issues_bridge_cli_accepts_minimum_configuration() {
    let mut cli = test_cli();
    cli.github_issues_bridge = true;
    cli.github_repo = Some("owner/repo".to_string());
    cli.github_token = Some("token".to_string());

    validate_github_issues_bridge_cli(&cli).expect("bridge config should validate");
}

#[test]
fn unit_validate_github_issues_bridge_cli_accepts_token_id_configuration() {
    let mut cli = test_cli();
    cli.github_issues_bridge = true;
    cli.github_repo = Some("owner/repo".to_string());
    cli.github_token_id = Some("github-token".to_string());

    validate_github_issues_bridge_cli(&cli).expect("bridge config should validate");
}

#[test]
fn functional_validate_github_issues_bridge_cli_rejects_prompt_conflicts() {
    let mut cli = test_cli();
    cli.github_issues_bridge = true;
    cli.github_repo = Some("owner/repo".to_string());
    cli.github_token = Some("token".to_string());
    cli.prompt = Some("conflict".to_string());

    let error = validate_github_issues_bridge_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--github-issues-bridge cannot be combined"));
}

#[test]
fn regression_validate_github_issues_bridge_cli_rejects_prompt_template_conflicts() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.github_issues_bridge = true;
    cli.github_repo = Some("owner/repo".to_string());
    cli.github_token = Some("token".to_string());
    cli.prompt_template_file = Some(temp.path().join("template.txt"));

    let error = validate_github_issues_bridge_cli(&cli).expect_err("template conflict");
    assert!(error.to_string().contains("--prompt-template-file"));
}

#[test]
fn regression_validate_github_issues_bridge_cli_requires_credentials() {
    let mut cli = test_cli();
    cli.github_issues_bridge = true;
    cli.github_repo = Some("owner/repo".to_string());
    cli.github_token = None;
    cli.github_token_id = None;

    let error = validate_github_issues_bridge_cli(&cli).expect_err("missing token");
    assert!(error
        .to_string()
        .contains("--github-token (or --github-token-id) is required"));
}

#[test]
fn unit_validate_slack_bridge_cli_accepts_minimum_configuration() {
    let mut cli = test_cli();
    cli.slack_bridge = true;
    cli.slack_app_token = Some("xapp-test".to_string());
    cli.slack_bot_token = Some("xoxb-test".to_string());

    validate_slack_bridge_cli(&cli).expect("slack bridge config should validate");
}

#[test]
fn unit_validate_slack_bridge_cli_accepts_token_id_configuration() {
    let mut cli = test_cli();
    cli.slack_bridge = true;
    cli.slack_app_token_id = Some("slack-app-token".to_string());
    cli.slack_bot_token_id = Some("slack-bot-token".to_string());

    validate_slack_bridge_cli(&cli).expect("slack bridge config should validate");
}

#[test]
fn functional_validate_slack_bridge_cli_rejects_prompt_conflicts() {
    let mut cli = test_cli();
    cli.slack_bridge = true;
    cli.slack_app_token = Some("xapp-test".to_string());
    cli.slack_bot_token = Some("xoxb-test".to_string());
    cli.prompt = Some("conflict".to_string());

    let error = validate_slack_bridge_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--slack-bridge cannot be combined"));
}

#[test]
fn regression_validate_slack_bridge_cli_rejects_prompt_template_conflicts() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.slack_bridge = true;
    cli.slack_app_token = Some("xapp-test".to_string());
    cli.slack_bot_token = Some("xoxb-test".to_string());
    cli.prompt_template_file = Some(temp.path().join("template.txt"));

    let error = validate_slack_bridge_cli(&cli).expect_err("template conflict");
    assert!(error.to_string().contains("--prompt-template-file"));
}

#[test]
fn regression_validate_slack_bridge_cli_rejects_missing_tokens() {
    let mut cli = test_cli();
    cli.slack_bridge = true;
    cli.slack_app_token = Some("xapp-test".to_string());
    cli.slack_bot_token = None;
    cli.slack_app_token_id = None;
    cli.slack_bot_token_id = None;

    let error = validate_slack_bridge_cli(&cli).expect_err("missing slack bot token");
    assert!(error
        .to_string()
        .contains("--slack-bot-token (or --slack-bot-token-id) is required"));
}

#[test]
fn unit_validate_events_runner_cli_accepts_minimum_configuration() {
    let mut cli = test_cli();
    cli.events_runner = true;
    validate_events_runner_cli(&cli).expect("events runner config should validate");
}

#[test]
fn functional_validate_events_runner_cli_rejects_prompt_conflicts() {
    let mut cli = test_cli();
    cli.events_runner = true;
    cli.prompt = Some("conflict".to_string());
    let error = validate_events_runner_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--events-runner cannot be combined"));
}

#[test]
fn regression_validate_events_runner_cli_rejects_prompt_template_conflicts() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.events_runner = true;
    cli.prompt_template_file = Some(temp.path().join("template.txt"));

    let error = validate_events_runner_cli(&cli).expect_err("template conflict");
    assert!(error.to_string().contains("--prompt-template-file"));
}

#[test]
fn regression_validate_event_webhook_ingest_cli_requires_channel() {
    let mut cli = test_cli();
    cli.event_webhook_ingest_file = Some(PathBuf::from("payload.json"));
    cli.event_webhook_channel = None;
    let error = validate_event_webhook_ingest_cli(&cli).expect_err("missing channel");
    assert!(error
        .to_string()
        .contains("--event-webhook-channel is required"));
}

#[test]
fn functional_validate_event_webhook_ingest_cli_requires_signing_arguments_together() {
    let mut cli = test_cli();
    cli.event_webhook_ingest_file = Some(PathBuf::from("payload.json"));
    cli.event_webhook_channel = Some("slack/C123".to_string());
    cli.event_webhook_signature = Some("sha256=abcd".to_string());
    cli.event_webhook_secret = Some("secret".to_string());

    let error = validate_event_webhook_ingest_cli(&cli).expect_err("algorithm should be required");
    assert!(error
        .to_string()
        .contains("--event-webhook-signature-algorithm is required"));
}

#[test]
fn functional_validate_event_webhook_ingest_cli_accepts_secret_id_configuration() {
    let mut cli = test_cli();
    cli.event_webhook_ingest_file = Some(PathBuf::from("payload.json"));
    cli.event_webhook_channel = Some("slack/C123".to_string());
    cli.event_webhook_signature = Some("sha256=abcd".to_string());
    cli.event_webhook_secret_id = Some("event-webhook-secret".to_string());
    cli.event_webhook_signature_algorithm = Some(CliWebhookSignatureAlgorithm::GithubSha256);

    validate_event_webhook_ingest_cli(&cli).expect("webhook config should validate");
}

#[test]
fn regression_validate_event_webhook_ingest_cli_requires_timestamp_for_slack_v0() {
    let mut cli = test_cli();
    cli.event_webhook_ingest_file = Some(PathBuf::from("payload.json"));
    cli.event_webhook_channel = Some("slack/C123".to_string());
    cli.event_webhook_signature = Some("v0=abcd".to_string());
    cli.event_webhook_secret = Some("secret".to_string());
    cli.event_webhook_signature_algorithm = Some(CliWebhookSignatureAlgorithm::SlackV0);
    cli.event_webhook_timestamp = None;

    let error = validate_event_webhook_ingest_cli(&cli).expect_err("timestamp should be required");
    assert!(error
        .to_string()
        .contains("--event-webhook-timestamp is required"));
}

#[test]
fn unit_validate_event_webhook_ingest_cli_accepts_signed_github_configuration() {
    let mut cli = test_cli();
    cli.event_webhook_ingest_file = Some(PathBuf::from("payload.json"));
    cli.event_webhook_channel = Some("github/owner/repo#1".to_string());
    cli.event_webhook_signature = Some("sha256=abcd".to_string());
    cli.event_webhook_secret = Some("secret".to_string());
    cli.event_webhook_signature_algorithm = Some(CliWebhookSignatureAlgorithm::GithubSha256);

    validate_event_webhook_ingest_cli(&cli).expect("signed github webhook config should pass");
}

#[test]
fn functional_execute_channel_store_admin_inspect_succeeds() {
    let temp = tempdir().expect("tempdir");
    let store = crate::channel_store::ChannelStore::open(temp.path(), "github", "issue-1")
        .expect("open channel store");
    store
        .append_log_entry(&crate::channel_store::ChannelLogEntry {
            timestamp_unix_ms: 1,
            direction: "inbound".to_string(),
            event_key: Some("e1".to_string()),
            source: "github".to_string(),
            payload: serde_json::json!({"body":"hello"}),
        })
        .expect("append log");
    store
        .write_text_artifact(
            "run-active",
            "github-reply",
            "private",
            Some(30),
            "md",
            "artifact body",
        )
        .expect("write artifact");
    let mut artifact_index =
        std::fs::read_to_string(store.artifact_index_path()).expect("read artifact index");
    artifact_index.push_str("invalid-artifact-line\n");
    std::fs::write(store.artifact_index_path(), artifact_index).expect("seed invalid artifact");

    let mut cli = test_cli();
    cli.channel_store_root = temp.path().to_path_buf();
    cli.channel_store_inspect = Some("github/issue-1".to_string());

    execute_channel_store_admin_command(&cli).expect("inspect should succeed");
    let report = store.inspect().expect("inspect report");
    assert_eq!(report.artifact_records, 1);
    assert_eq!(report.invalid_artifact_lines, 1);
    assert_eq!(report.active_artifacts, 1);
    assert_eq!(report.expired_artifacts, 0);
}

#[test]
fn regression_execute_channel_store_admin_repair_removes_invalid_lines() {
    let temp = tempdir().expect("tempdir");
    let store = crate::channel_store::ChannelStore::open(temp.path(), "slack", "C123")
        .expect("open channel store");
    std::fs::write(store.log_path(), "{\"ok\":true}\ninvalid-json-line\n")
        .expect("seed invalid log");
    let expired = store
        .write_text_artifact(
            "run-expired",
            "slack-reply",
            "private",
            Some(0),
            "md",
            "expired artifact",
        )
        .expect("write expired artifact");
    let mut artifact_index =
        std::fs::read_to_string(store.artifact_index_path()).expect("read artifact index");
    artifact_index.push_str("invalid-artifact-line\n");
    std::fs::write(store.artifact_index_path(), artifact_index).expect("seed invalid artifact");

    let mut cli = test_cli();
    cli.channel_store_root = temp.path().to_path_buf();
    cli.channel_store_repair = Some("slack/C123".to_string());
    execute_channel_store_admin_command(&cli).expect("repair should succeed");

    let report = store.inspect().expect("inspect after repair");
    assert_eq!(report.invalid_log_lines, 0);
    assert_eq!(report.log_records, 1);
    assert_eq!(report.invalid_artifact_lines, 0);
    assert_eq!(report.expired_artifacts, 0);
    assert_eq!(report.active_artifacts, 0);
    assert!(!store.channel_dir().join(expired.relative_path).exists());
}

fn make_script_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions).expect("set executable permissions");
    }
}

#[test]
fn functional_execute_extension_validate_command_succeeds_for_valid_manifest() {
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
  "hooks": ["run-start", "run-end"],
  "permissions": ["read-files", "network"],
  "timeout_ms": 60000
}"#,
    )
    .expect("write extension manifest");

    let mut cli = test_cli();
    cli.extension_validate = Some(manifest_path);
    execute_extension_validate_command(&cli).expect("extension validate should succeed");
}

#[test]
fn regression_execute_extension_validate_command_rejects_invalid_manifest() {
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
    .expect("write extension manifest");

    let mut cli = test_cli();
    cli.extension_validate = Some(manifest_path);
    let error =
        execute_extension_validate_command(&cli).expect_err("unsafe entrypoint should fail");
    assert!(error
        .to_string()
        .contains("must not contain parent traversals"));
}

#[test]
fn functional_execute_extension_show_command_succeeds_for_valid_manifest() {
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
  "hooks": ["run-end", "run-start"],
  "permissions": ["network", "read-files"]
}"#,
    )
    .expect("write extension manifest");

    let mut cli = test_cli();
    cli.extension_show = Some(manifest_path);
    execute_extension_show_command(&cli).expect("extension show should succeed");
}

#[test]
fn regression_execute_extension_show_command_rejects_invalid_manifest() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("extension.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 9,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/assistant"
}"#,
    )
    .expect("write extension manifest");

    let mut cli = test_cli();
    cli.extension_show = Some(manifest_path);
    let error = execute_extension_show_command(&cli).expect_err("invalid schema should fail");
    assert!(error
        .to_string()
        .contains("unsupported extension manifest schema"));
}

#[test]
fn functional_execute_extension_list_command_reports_mixed_inventory() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("extensions");
    let valid_dir = root.join("issue-assistant");
    std::fs::create_dir_all(&valid_dir).expect("create valid extension dir");
    std::fs::write(
        valid_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/assistant"
}"#,
    )
    .expect("write valid extension manifest");
    let invalid_dir = root.join("broken");
    std::fs::create_dir_all(&invalid_dir).expect("create invalid extension dir");
    std::fs::write(
        invalid_dir.join("extension.json"),
        r#"{
  "schema_version": 9,
  "id": "broken",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/assistant"
}"#,
    )
    .expect("write invalid extension manifest");

    let mut cli = test_cli();
    cli.extension_list = true;
    cli.extension_list_root = root;
    execute_extension_list_command(&cli).expect("extension list should succeed");
}

#[test]
fn regression_execute_extension_list_command_rejects_non_directory_root() {
    let temp = tempdir().expect("tempdir");
    let root_file = temp.path().join("extensions.json");
    std::fs::write(&root_file, "{}").expect("write root file");

    let mut cli = test_cli();
    cli.extension_list = true;
    cli.extension_list_root = root_file;
    let error =
        execute_extension_list_command(&cli).expect_err("non-directory extension root should fail");
    assert!(error.to_string().contains("is not a directory"));
}

#[test]
fn functional_execute_extension_exec_command_runs_process_hook() {
    let temp = tempdir().expect("tempdir");
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir).expect("create bin directory");
    let script_path = bin_dir.join("hook.sh");
    std::fs::write(
        &script_path,
        "#!/usr/bin/env bash\nread -r _input\nprintf '{\"ok\":true,\"result\":\"hook-processed\"}'\n",
    )
    .expect("write hook script");
    make_script_executable(&script_path);
    let manifest_path = temp.path().join("extension.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/hook.sh",
  "hooks": ["run-start"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write manifest");
    let payload_path = temp.path().join("payload.json");
    std::fs::write(&payload_path, r#"{"event":"created"}"#).expect("write payload");

    let mut cli = test_cli();
    cli.extension_exec_manifest = Some(manifest_path);
    cli.extension_exec_hook = Some("run-start".to_string());
    cli.extension_exec_payload_file = Some(payload_path);
    execute_extension_exec_command(&cli).expect("extension exec should succeed");
}

#[test]
fn regression_execute_extension_exec_command_rejects_undeclared_hook() {
    let temp = tempdir().expect("tempdir");
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir).expect("create bin directory");
    let script_path = bin_dir.join("hook.sh");
    std::fs::write(
        &script_path,
        "#!/usr/bin/env bash\nread -r _input\nprintf '{\"ok\":true}'\n",
    )
    .expect("write hook script");
    make_script_executable(&script_path);
    let manifest_path = temp.path().join("extension.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/hook.sh",
  "hooks": ["run-end"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write manifest");
    let payload_path = temp.path().join("payload.json");
    std::fs::write(&payload_path, r#"{"event":"created"}"#).expect("write payload");

    let mut cli = test_cli();
    cli.extension_exec_manifest = Some(manifest_path);
    cli.extension_exec_hook = Some("run-start".to_string());
    cli.extension_exec_payload_file = Some(payload_path);
    let error = execute_extension_exec_command(&cli).expect_err("undeclared hook should fail");
    assert!(error.to_string().contains("does not declare hook"));
}

#[test]
fn regression_execute_extension_exec_command_enforces_timeout() {
    let temp = tempdir().expect("tempdir");
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir).expect("create bin directory");
    let script_path = bin_dir.join("slow.sh");
    std::fs::write(
        &script_path,
        "#!/usr/bin/env bash\nsleep 1\nprintf '{\"ok\":true}'\n",
    )
    .expect("write hook script");
    make_script_executable(&script_path);
    let manifest_path = temp.path().join("extension.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/slow.sh",
  "hooks": ["run-start"],
  "permissions": ["run-commands"],
  "timeout_ms": 20
}"#,
    )
    .expect("write manifest");
    let payload_path = temp.path().join("payload.json");
    std::fs::write(&payload_path, r#"{"event":"created"}"#).expect("write payload");

    let mut cli = test_cli();
    cli.extension_exec_manifest = Some(manifest_path);
    cli.extension_exec_hook = Some("run-start".to_string());
    cli.extension_exec_payload_file = Some(payload_path);
    let error = execute_extension_exec_command(&cli).expect_err("timeout should fail");
    assert!(error.to_string().contains("timed out"));
}

#[test]
fn regression_execute_extension_exec_command_rejects_invalid_json_response() {
    let temp = tempdir().expect("tempdir");
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir).expect("create bin directory");
    let script_path = bin_dir.join("bad.sh");
    std::fs::write(
        &script_path,
        "#!/usr/bin/env bash\nread -r _input\nprintf 'not-json'\n",
    )
    .expect("write hook script");
    make_script_executable(&script_path);
    let manifest_path = temp.path().join("extension.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/bad.sh",
  "hooks": ["run-start"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write manifest");
    let payload_path = temp.path().join("payload.json");
    std::fs::write(&payload_path, r#"{"event":"created"}"#).expect("write payload");

    let mut cli = test_cli();
    cli.extension_exec_manifest = Some(manifest_path);
    cli.extension_exec_hook = Some("run-start".to_string());
    cli.extension_exec_payload_file = Some(payload_path);
    let error =
        execute_extension_exec_command(&cli).expect_err("invalid output should be rejected");
    assert!(error.to_string().contains("response must be valid JSON"));
}

#[test]
fn functional_execute_package_validate_command_succeeds_for_valid_manifest() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let mut cli = test_cli();
    cli.package_validate = Some(manifest_path);
    execute_package_validate_command(&cli).expect("package validate should succeed");
}

#[test]
fn regression_execute_package_validate_command_rejects_invalid_manifest() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 9,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let mut cli = test_cli();
    cli.package_validate = Some(manifest_path);
    let error = execute_package_validate_command(&cli).expect_err("invalid schema should fail");
    assert!(error
        .to_string()
        .contains("unsupported package manifest schema"));
}

#[test]
fn functional_execute_package_show_command_succeeds_for_valid_manifest() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}],
  "skills": [{"id":"checks","path":"skills/checks/SKILL.md"}]
}"#,
    )
    .expect("write manifest");

    let mut cli = test_cli();
    cli.package_show = Some(manifest_path);
    execute_package_show_command(&cli).expect("package show should succeed");
}

#[test]
fn regression_execute_package_show_command_rejects_invalid_manifest() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "invalid",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let mut cli = test_cli();
    cli.package_show = Some(manifest_path);
    let error = execute_package_show_command(&cli).expect_err("invalid version should fail");
    assert!(error.to_string().contains("must follow x.y.z"));
}

#[test]
fn functional_execute_package_install_command_succeeds_for_valid_manifest() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    std::fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    std::fs::write(package_root.join("templates/review.txt"), "template")
        .expect("write template source");

    let manifest_path = package_root.join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let install_root = temp.path().join("installed");
    let mut cli = test_cli();
    cli.package_install = Some(manifest_path);
    cli.package_install_root = install_root.clone();

    execute_package_install_command(&cli).expect("package install should succeed");
    assert!(install_root
        .join("starter-bundle/1.0.0/templates/review.txt")
        .exists());
}

#[test]
fn functional_execute_package_install_command_supports_remote_sources_with_checksum() {
    let server = MockServer::start();
    let remote_body = "remote template body";
    let remote_mock = server.mock(|when, then| {
        when.method(GET).path("/templates/review.txt");
        then.status(200).body(remote_body);
    });

    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    std::fs::create_dir_all(&package_root).expect("create package root");
    let checksum = format!("{:x}", Sha256::digest(remote_body.as_bytes()));
    let manifest_path = package_root.join("package.json");
    std::fs::write(
        &manifest_path,
        format!(
            r#"{{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{{
    "id":"review",
    "path":"templates/review.txt",
    "url":"{}/templates/review.txt",
    "sha256":"sha256:{checksum}"
  }}]
}}"#,
            server.base_url()
        ),
    )
    .expect("write manifest");

    let install_root = temp.path().join("installed");
    let mut cli = test_cli();
    cli.package_install = Some(manifest_path);
    cli.package_install_root = install_root.clone();
    execute_package_install_command(&cli).expect("package install should succeed");
    assert_eq!(
        std::fs::read_to_string(install_root.join("starter-bundle/1.0.0/templates/review.txt"))
            .expect("read installed template"),
        remote_body
    );
    remote_mock.assert();
}

#[test]
fn regression_execute_package_install_command_rejects_missing_component_source() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    std::fs::create_dir_all(package_root.join("templates")).expect("create templates dir");

    let manifest_path = package_root.join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/missing.txt"}]
}"#,
    )
    .expect("write manifest");

    let mut cli = test_cli();
    cli.package_install = Some(manifest_path);
    cli.package_install_root = temp.path().join("installed");
    let error = execute_package_install_command(&cli).expect_err("missing source should fail");
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn regression_execute_package_install_command_rejects_remote_checksum_mismatch() {
    let server = MockServer::start();
    let remote_mock = server.mock(|when, then| {
        when.method(GET).path("/templates/review.txt");
        then.status(200).body("remote template");
    });

    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    std::fs::create_dir_all(&package_root).expect("create package root");
    let manifest_path = package_root.join("package.json");
    std::fs::write(
        &manifest_path,
        format!(
            r#"{{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{{
    "id":"review",
    "path":"templates/review.txt",
    "url":"{}/templates/review.txt",
    "sha256":"sha256:{}"
  }}]
}}"#,
            server.base_url(),
            "0".repeat(64)
        ),
    )
    .expect("write manifest");

    let mut cli = test_cli();
    cli.package_install = Some(manifest_path);
    cli.package_install_root = temp.path().join("installed");
    let error =
        execute_package_install_command(&cli).expect_err("checksum mismatch should fail install");
    assert!(error.to_string().contains("checksum mismatch"));
    remote_mock.assert();
}

#[test]
fn regression_execute_package_install_command_rejects_unsigned_when_required() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    std::fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    std::fs::write(package_root.join("templates/review.txt"), "template")
        .expect("write template source");
    let manifest_path = package_root.join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let mut cli = test_cli();
    cli.package_install = Some(manifest_path);
    cli.package_install_root = temp.path().join("installed");
    cli.require_signed_packages = true;
    let error =
        execute_package_install_command(&cli).expect_err("unsigned package should fail policy");
    assert!(error
        .to_string()
        .contains("must include signing_key and signature_file"));
}

#[test]
fn functional_execute_package_update_command_updates_existing_package() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    std::fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    let template_path = package_root.join("templates/review.txt");
    std::fs::write(&template_path, "template-v1").expect("write template source");
    let manifest_path = package_root.join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let install_root = temp.path().join("installed");
    let mut install_cli = test_cli();
    install_cli.package_install = Some(manifest_path.clone());
    install_cli.package_install_root = install_root.clone();
    execute_package_install_command(&install_cli).expect("package install should succeed");

    std::fs::write(&template_path, "template-v2").expect("update template source");
    let mut update_cli = test_cli();
    update_cli.package_update = Some(manifest_path);
    update_cli.package_update_root = install_root.clone();
    execute_package_update_command(&update_cli).expect("package update should succeed");
    assert_eq!(
        std::fs::read_to_string(install_root.join("starter-bundle/1.0.0/templates/review.txt"))
            .expect("read updated template"),
        "template-v2"
    );
}

#[test]
fn regression_execute_package_update_command_rejects_missing_target() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    std::fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    std::fs::write(package_root.join("templates/review.txt"), "template")
        .expect("write template source");
    let manifest_path = package_root.join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let mut cli = test_cli();
    cli.package_update = Some(manifest_path);
    cli.package_update_root = temp.path().join("installed");
    let error = execute_package_update_command(&cli).expect_err("missing target should fail");
    assert!(error.to_string().contains("is not installed"));
}

#[test]
fn functional_execute_package_list_command_reports_installed_packages() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    std::fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    std::fs::write(package_root.join("templates/review.txt"), "template")
        .expect("write template source");
    let manifest_path = package_root.join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let install_root = temp.path().join("installed");
    let mut install_cli = test_cli();
    install_cli.package_install = Some(manifest_path);
    install_cli.package_install_root = install_root.clone();
    execute_package_install_command(&install_cli).expect("package install should succeed");

    let mut list_cli = test_cli();
    list_cli.package_list = true;
    list_cli.package_list_root = install_root;
    execute_package_list_command(&list_cli).expect("package list should succeed");
}

#[test]
fn regression_execute_package_list_command_rejects_non_directory_root() {
    let temp = tempdir().expect("tempdir");
    let root_file = temp.path().join("not-a-directory.txt");
    std::fs::write(&root_file, "file root").expect("write root file");

    let mut cli = test_cli();
    cli.package_list = true;
    cli.package_list_root = root_file;
    let error = execute_package_list_command(&cli).expect_err("non-directory root should fail");
    assert!(error.to_string().contains("is not a directory"));
}

#[test]
fn functional_execute_package_remove_command_removes_installed_package() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    std::fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    std::fs::write(package_root.join("templates/review.txt"), "template")
        .expect("write template source");
    let manifest_path = package_root.join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let install_root = temp.path().join("installed");
    let mut install_cli = test_cli();
    install_cli.package_install = Some(manifest_path);
    install_cli.package_install_root = install_root.clone();
    execute_package_install_command(&install_cli).expect("package install should succeed");

    let mut remove_cli = test_cli();
    remove_cli.package_remove = Some("starter-bundle@1.0.0".to_string());
    remove_cli.package_remove_root = install_root.clone();
    execute_package_remove_command(&remove_cli).expect("package remove should succeed");
    assert!(!install_root.join("starter-bundle/1.0.0").exists());
}

#[test]
fn regression_execute_package_remove_command_rejects_invalid_coordinate() {
    let mut cli = test_cli();
    cli.package_remove = Some("starter-bundle".to_string());
    cli.package_remove_root = PathBuf::from(".tau/packages");
    let error =
        execute_package_remove_command(&cli).expect_err("invalid coordinate format should fail");
    assert!(error.to_string().contains("must follow <name>@<version>"));
}

#[test]
fn functional_execute_package_rollback_command_removes_non_target_versions() {
    let temp = tempdir().expect("tempdir");
    let install_root = temp.path().join("installed");
    let install_version = |version: &str, body: &str| {
        let source_root = temp.path().join(format!("bundle-{version}"));
        std::fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
        std::fs::write(source_root.join("templates/review.txt"), body)
            .expect("write template source");
        let manifest_path = source_root.join("package.json");
        std::fs::write(
            &manifest_path,
            format!(
                r#"{{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "{version}",
  "templates": [{{"id":"review","path":"templates/review.txt"}}]
}}"#
            ),
        )
        .expect("write manifest");

        let mut install_cli = test_cli();
        install_cli.package_install = Some(manifest_path);
        install_cli.package_install_root = install_root.clone();
        execute_package_install_command(&install_cli).expect("package install should succeed");
    };

    install_version("1.0.0", "v1");
    install_version("2.0.0", "v2");

    let mut rollback_cli = test_cli();
    rollback_cli.package_rollback = Some("starter-bundle@1.0.0".to_string());
    rollback_cli.package_rollback_root = install_root.clone();
    execute_package_rollback_command(&rollback_cli).expect("package rollback should succeed");
    assert!(install_root.join("starter-bundle/1.0.0").exists());
    assert!(!install_root.join("starter-bundle/2.0.0").exists());
}

#[test]
fn regression_execute_package_rollback_command_rejects_missing_target() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.package_rollback = Some("starter-bundle@1.0.0".to_string());
    cli.package_rollback_root = temp.path().join("installed");
    let error =
        execute_package_rollback_command(&cli).expect_err("missing target version should fail");
    assert!(error.to_string().contains("is not installed"));
}

#[test]
fn regression_execute_package_rollback_command_rejects_invalid_coordinate() {
    let mut cli = test_cli();
    cli.package_rollback = Some("../starter@1.0.0".to_string());
    cli.package_rollback_root = PathBuf::from(".tau/packages");
    let error = execute_package_rollback_command(&cli).expect_err("invalid coordinate should fail");
    assert!(error
        .to_string()
        .contains("must not contain path separators"));
}

#[test]
fn functional_execute_package_conflicts_command_reports_detected_collisions() {
    let temp = tempdir().expect("tempdir");
    let install_root = temp.path().join("installed");
    let install_package = |name: &str, body: &str| {
        let source_root = temp.path().join(format!("bundle-{name}"));
        std::fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
        std::fs::write(source_root.join("templates/review.txt"), body)
            .expect("write template source");
        let manifest_path = source_root.join("package.json");
        std::fs::write(
            &manifest_path,
            format!(
                r#"{{
  "schema_version": 1,
  "name": "{name}",
  "version": "1.0.0",
  "templates": [{{"id":"review","path":"templates/review.txt"}}]
}}"#
            ),
        )
        .expect("write manifest");
        let mut install_cli = test_cli();
        install_cli.package_install = Some(manifest_path);
        install_cli.package_install_root = install_root.clone();
        execute_package_install_command(&install_cli).expect("package install should succeed");
    };

    install_package("alpha", "alpha body");
    install_package("zeta", "zeta body");

    let mut cli = test_cli();
    cli.package_conflicts = true;
    cli.package_conflicts_root = install_root;
    execute_package_conflicts_command(&cli).expect("package conflicts should succeed");
}

#[test]
fn regression_execute_package_conflicts_command_rejects_non_directory_root() {
    let temp = tempdir().expect("tempdir");
    let root_file = temp.path().join("not-a-directory.txt");
    std::fs::write(&root_file, "file root").expect("write root file");

    let mut cli = test_cli();
    cli.package_conflicts = true;
    cli.package_conflicts_root = root_file;
    let error =
        execute_package_conflicts_command(&cli).expect_err("non-directory root should fail");
    assert!(error.to_string().contains("is not a directory"));
}

#[test]
fn functional_execute_package_activate_command_materializes_components() {
    let temp = tempdir().expect("tempdir");
    let install_root = temp.path().join("installed");
    let source_root = temp.path().join("bundle");
    std::fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
    std::fs::create_dir_all(source_root.join("skills/checks")).expect("create skills dir");
    std::fs::write(source_root.join("templates/review.txt"), "template body")
        .expect("write template source");
    std::fs::write(source_root.join("skills/checks/SKILL.md"), "# checks")
        .expect("write skill source");
    let manifest_path = source_root.join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}],
  "skills": [{"id":"checks","path":"skills/checks/SKILL.md"}]
}"#,
    )
    .expect("write manifest");

    let mut install_cli = test_cli();
    install_cli.package_install = Some(manifest_path);
    install_cli.package_install_root = install_root.clone();
    execute_package_install_command(&install_cli).expect("package install should succeed");

    let destination_root = temp.path().join("activated");
    let mut cli = test_cli();
    cli.package_activate = true;
    cli.package_activate_root = install_root;
    cli.package_activate_destination = destination_root.clone();
    execute_package_activate_command(&cli).expect("package activate should succeed");
    assert_eq!(
        std::fs::read_to_string(destination_root.join("templates/review.txt"))
            .expect("read activated template"),
        "template body"
    );
    assert_eq!(
        std::fs::read_to_string(destination_root.join("skills/checks/SKILL.md"))
            .expect("read activated skill"),
        "# checks"
    );
}

#[test]
fn regression_execute_package_activate_command_rejects_unsupported_conflict_policy() {
    let mut cli = test_cli();
    cli.package_activate = true;
    cli.package_activate_conflict_policy = "unsupported".to_string();
    let error = execute_package_activate_command(&cli)
        .expect_err("unsupported conflict policy should fail");
    assert!(error
        .to_string()
        .contains("unsupported package activation conflict policy"));
}

#[test]
fn regression_execute_package_activate_on_startup_is_noop_when_disabled() {
    let temp = tempdir().expect("tempdir");
    let destination_root = temp.path().join("activated");
    let mut cli = test_cli();
    cli.package_activate_root = temp.path().join("installed");
    cli.package_activate_destination = destination_root.clone();
    let report = execute_package_activate_on_startup(&cli)
        .expect("startup activation should allow disabled mode");
    assert!(report.is_none());
    assert!(!destination_root.exists());
}

#[test]
fn functional_execute_package_activate_on_startup_creates_runtime_skill_alias() {
    let temp = tempdir().expect("tempdir");
    let install_root = temp.path().join("installed");
    let source_root = temp.path().join("bundle");
    std::fs::create_dir_all(source_root.join("skills/checks")).expect("create skills dir");
    std::fs::write(source_root.join("skills/checks/SKILL.md"), "# checks")
        .expect("write skill source");
    let manifest_path = source_root.join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "skills": [{"id":"checks","path":"skills/checks/SKILL.md"}]
}"#,
    )
    .expect("write manifest");

    let mut install_cli = test_cli();
    install_cli.package_install = Some(manifest_path);
    install_cli.package_install_root = install_root.clone();
    execute_package_install_command(&install_cli).expect("package install should succeed");

    let destination_root = temp.path().join("activated");
    let mut cli = test_cli();
    cli.package_activate_on_startup = true;
    cli.package_activate_root = install_root;
    cli.package_activate_destination = destination_root.clone();
    let report = execute_package_activate_on_startup(&cli)
        .expect("startup activation should succeed")
        .expect("startup activation should return report");
    assert_eq!(report.activated_components, 1);
    assert_eq!(
        std::fs::read_to_string(destination_root.join("skills/checks/SKILL.md"))
            .expect("read activated nested skill"),
        "# checks"
    );
    assert_eq!(
        std::fs::read_to_string(destination_root.join("skills/checks.md"))
            .expect("read activated skill alias"),
        "# checks"
    );
}

#[test]
fn integration_compose_startup_system_prompt_uses_activated_skill_aliases() {
    let temp = tempdir().expect("tempdir");
    let install_root = temp.path().join("installed");
    let source_root = temp.path().join("bundle");
    std::fs::create_dir_all(source_root.join("skills/checks")).expect("create skills dir");
    std::fs::write(
        source_root.join("skills/checks/SKILL.md"),
        "Always run tests",
    )
    .expect("write skill source");
    let manifest_path = source_root.join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "skills": [{"id":"checks","path":"skills/checks/SKILL.md"}]
}"#,
    )
    .expect("write manifest");

    let mut install_cli = test_cli();
    install_cli.package_install = Some(manifest_path);
    install_cli.package_install_root = install_root.clone();
    execute_package_install_command(&install_cli).expect("package install should succeed");

    let destination_root = temp.path().join("activated");
    let mut activation_cli = test_cli();
    activation_cli.package_activate_on_startup = true;
    activation_cli.package_activate_root = install_root;
    activation_cli.package_activate_destination = destination_root.clone();
    execute_package_activate_on_startup(&activation_cli)
        .expect("startup activation should succeed");

    let mut cli = test_cli();
    cli.system_prompt = "base prompt".to_string();
    cli.skills = vec!["checks".to_string()];
    let composed = compose_startup_system_prompt(&cli, &destination_root.join("skills"))
        .expect("compose startup prompt");
    assert!(composed.contains("base prompt"));
    assert!(composed.contains("Always run tests"));
}

#[test]
fn regression_execute_package_activate_on_startup_rejects_unsupported_conflict_policy() {
    let mut cli = test_cli();
    cli.package_activate_on_startup = true;
    cli.package_activate_conflict_policy = "unsupported".to_string();
    let error = execute_package_activate_on_startup(&cli)
        .expect_err("unsupported conflict policy should fail");
    assert!(error
        .to_string()
        .contains("unsupported package activation conflict policy"));
}

#[test]
fn unit_rpc_capabilities_payload_includes_protocol_and_capabilities() {
    let payload = rpc_capabilities_payload();
    assert_eq!(payload["schema_version"].as_u64(), Some(1));
    assert_eq!(payload["protocol_version"].as_str(), Some("0.1.0"));
    let capabilities = payload["capabilities"]
        .as_array()
        .expect("capabilities should be array");
    assert!(capabilities.iter().any(|entry| entry == "run.start"));
    assert!(capabilities.iter().any(|entry| entry == "run.cancel"));
}

#[test]
fn functional_execute_rpc_capabilities_command_succeeds_when_enabled() {
    let mut cli = test_cli();
    cli.rpc_capabilities = true;
    execute_rpc_capabilities_command(&cli).expect("rpc capabilities command should succeed");
}

#[test]
fn regression_execute_rpc_capabilities_command_is_noop_when_disabled() {
    let cli = test_cli();
    execute_rpc_capabilities_command(&cli).expect("disabled rpc capabilities should be noop");
}

#[test]
fn unit_validate_rpc_frame_file_parses_supported_frame_shape() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frame.json");
    std::fs::write(
        &frame_path,
        r#"{
  "schema_version": 1,
  "request_id": "req-1",
  "kind": "run.start",
  "payload": {"prompt":"hello"}
}"#,
    )
    .expect("write frame");
    let frame = validate_rpc_frame_file(&frame_path).expect("validate frame");
    assert_eq!(frame.request_id, "req-1");
    assert_eq!(frame.payload.len(), 1);
}

#[test]
fn functional_execute_rpc_validate_frame_command_succeeds_for_valid_frame() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frame.json");
    std::fs::write(
        &frame_path,
        r#"{
  "schema_version": 1,
  "request_id": "req-cancel",
  "kind": "run.cancel",
  "payload": {"run_id":"run-1"}
}"#,
    )
    .expect("write frame");
    let mut cli = test_cli();
    cli.rpc_validate_frame_file = Some(frame_path);
    execute_rpc_validate_frame_command(&cli).expect("rpc frame validate should succeed");
}

#[test]
fn regression_execute_rpc_validate_frame_command_rejects_invalid_frame() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frame.json");
    std::fs::write(
        &frame_path,
        r#"{
  "schema_version": 1,
  "request_id": "req-invalid",
  "kind": "run.unknown",
  "payload": {}
}"#,
    )
    .expect("write frame");
    let mut cli = test_cli();
    cli.rpc_validate_frame_file = Some(frame_path);
    let error = execute_rpc_validate_frame_command(&cli).expect_err("invalid kind should fail");
    assert!(error.to_string().contains("unsupported rpc frame kind"));
}

#[test]
fn functional_execute_rpc_dispatch_frame_command_succeeds_for_valid_frame() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frame.json");
    std::fs::write(
        &frame_path,
        r#"{
  "schema_version": 1,
  "request_id": "req-dispatch",
  "kind": "run.cancel",
  "payload": {"run_id":"run-1"}
}"#,
    )
    .expect("write frame");
    let mut cli = test_cli();
    cli.rpc_dispatch_frame_file = Some(frame_path);
    execute_rpc_dispatch_frame_command(&cli).expect("rpc frame dispatch should succeed");
}

#[test]
fn regression_execute_rpc_dispatch_frame_command_rejects_missing_prompt() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frame.json");
    std::fs::write(
        &frame_path,
        r#"{
  "schema_version": 1,
  "request_id": "req-start",
  "kind": "run.start",
  "payload": {}
}"#,
    )
    .expect("write frame");
    let mut cli = test_cli();
    cli.rpc_dispatch_frame_file = Some(frame_path);
    let error = execute_rpc_dispatch_frame_command(&cli).expect_err("missing prompt should fail");
    assert!(error
        .to_string()
        .contains("requires non-empty payload field 'prompt'"));
}

#[test]
fn functional_execute_rpc_dispatch_ndjson_command_succeeds_for_valid_frames() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frames.ndjson");
    std::fs::write(
        &frame_path,
        r#"{"schema_version":1,"request_id":"req-cap","kind":"capabilities.request","payload":{}}
{"schema_version":1,"request_id":"req-start","kind":"run.start","payload":{"prompt":"hello"}}
"#,
    )
    .expect("write frames");
    let mut cli = test_cli();
    cli.rpc_dispatch_ndjson_file = Some(frame_path);
    execute_rpc_dispatch_ndjson_command(&cli).expect("rpc ndjson dispatch should succeed");
}

#[test]
fn regression_execute_rpc_dispatch_ndjson_command_fails_with_any_error_frame() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frames.ndjson");
    std::fs::write(
        &frame_path,
        r#"{"schema_version":1,"request_id":"req-cap","kind":"capabilities.request","payload":{}}
not-json
"#,
    )
    .expect("write frames");
    let mut cli = test_cli();
    cli.rpc_dispatch_ndjson_file = Some(frame_path);
    let error = execute_rpc_dispatch_ndjson_command(&cli)
        .expect_err("mixed ndjson frames should return an error");
    assert!(error
        .to_string()
        .contains("rpc ndjson dispatch completed with 1 error frame(s)"));
}

#[test]
fn regression_execute_rpc_serve_ndjson_command_is_noop_when_disabled() {
    let cli = test_cli();
    execute_rpc_serve_ndjson_command(&cli).expect("disabled rpc ndjson serve should be noop");
}

#[test]
fn unit_resolve_system_prompt_uses_inline_value_when_file_is_unset() {
    let mut cli = test_cli();
    cli.system_prompt = "inline system".to_string();

    let system_prompt = resolve_system_prompt(&cli).expect("resolve system prompt");
    assert_eq!(system_prompt, "inline system");
}

#[test]
fn functional_resolve_system_prompt_reads_system_prompt_file() {
    let temp = tempdir().expect("tempdir");
    let prompt_path = temp.path().join("system.txt");
    std::fs::write(&prompt_path, "system from file").expect("write prompt");

    let mut cli = test_cli();
    cli.system_prompt_file = Some(prompt_path);

    let system_prompt = resolve_system_prompt(&cli).expect("resolve system prompt");
    assert_eq!(system_prompt, "system from file");
}

#[test]
fn regression_resolve_system_prompt_rejects_empty_system_prompt_file() {
    let temp = tempdir().expect("tempdir");
    let prompt_path = temp.path().join("system.txt");
    std::fs::write(&prompt_path, "\n\t  ").expect("write prompt");

    let mut cli = test_cli();
    cli.system_prompt_file = Some(prompt_path.clone());

    let error = resolve_system_prompt(&cli).expect_err("empty system prompt should fail");
    assert!(error.to_string().contains(&format!(
        "system prompt file {} is empty",
        prompt_path.display()
    )));
}

#[test]
fn pathbuf_from_cli_default_is_relative() {
    let path = PathBuf::from(".tau/sessions/default.jsonl");
    assert!(!path.is_absolute());
}

#[test]
fn unit_parse_trusted_root_spec_accepts_key_id_and_base64() {
    let parsed = parse_trusted_root_spec("root=ZmFrZS1rZXk=").expect("parse root");
    assert_eq!(parsed.id, "root");
    assert_eq!(parsed.public_key, "ZmFrZS1rZXk=");
}

#[test]
fn regression_parse_trusted_root_spec_rejects_invalid_shapes() {
    let error = parse_trusted_root_spec("missing-separator").expect_err("should fail");
    assert!(error.to_string().contains("expected key_id=base64_key"));
}

#[test]
fn unit_parse_trust_rotation_spec_accepts_old_and_new_key() {
    let (old_id, new_key) = parse_trust_rotation_spec("old:new=YQ==").expect("rotation spec parse");
    assert_eq!(old_id, "old");
    assert_eq!(new_key.id, "new");
    assert_eq!(new_key.public_key, "YQ==");
}

#[test]
fn regression_parse_trust_rotation_spec_rejects_invalid_shapes() {
    let error = parse_trust_rotation_spec("invalid-shape").expect_err("should fail");
    assert!(error
        .to_string()
        .contains("expected old_id:new_id=base64_key"));
}

#[test]
fn functional_apply_trust_root_mutations_add_revoke_and_rotate() {
    let mut records = vec![TrustedRootRecord {
        id: "old".to_string(),
        public_key: "YQ==".to_string(),
        revoked: false,
        expires_unix: None,
        rotated_from: None,
    }];
    let mut cli = test_cli();
    cli.skill_trust_add = vec!["extra=Yg==".to_string()];
    cli.skill_trust_revoke = vec!["extra".to_string()];
    cli.skill_trust_rotate = vec!["old:new=Yw==".to_string()];

    let report = apply_trust_root_mutations(&mut records, &cli).expect("mutate");
    assert_eq!(report.added, 2);
    assert_eq!(report.updated, 0);
    assert_eq!(report.revoked, 1);
    assert_eq!(report.rotated, 1);

    let old = records
        .iter()
        .find(|record| record.id == "old")
        .expect("old");
    let new = records
        .iter()
        .find(|record| record.id == "new")
        .expect("new");
    let extra = records
        .iter()
        .find(|record| record.id == "extra")
        .expect("extra");
    assert!(old.revoked);
    assert_eq!(new.rotated_from.as_deref(), Some("old"));
    assert!(extra.revoked);
}

#[test]
fn functional_resolve_skill_trust_roots_loads_inline_and_file_entries() {
    let temp = tempdir().expect("tempdir");
    let roots_file = temp.path().join("roots.json");
    std::fs::write(
        &roots_file,
        r#"{"roots":[{"id":"file-root","public_key":"YQ=="}]}"#,
    )
    .expect("write roots");

    let mut cli = test_cli();
    cli.skill_trust_root = vec!["inline-root=Yg==".to_string()];
    cli.skill_trust_root_file = Some(roots_file);

    let roots = resolve_skill_trust_roots(&cli).expect("resolve roots");
    assert_eq!(roots.len(), 2);
    assert_eq!(roots[0].id, "inline-root");
    assert_eq!(roots[1].id, "file-root");
}

#[test]
fn integration_resolve_skill_trust_roots_applies_mutations_and_persists_file() {
    let temp = tempdir().expect("tempdir");
    let roots_file = temp.path().join("roots.json");
    std::fs::write(
        &roots_file,
        r#"{"roots":[{"id":"old","public_key":"YQ=="}]}"#,
    )
    .expect("write roots");

    let mut cli = test_cli();
    cli.skill_trust_root_file = Some(roots_file.clone());
    cli.skill_trust_rotate = vec!["old:new=Yg==".to_string()];

    let roots = resolve_skill_trust_roots(&cli).expect("resolve roots");
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].id, "new");

    let raw = std::fs::read_to_string(&roots_file).expect("read persisted");
    assert!(raw.contains("\"id\": \"old\""));
    assert!(raw.contains("\"revoked\": true"));
    assert!(raw.contains("\"id\": \"new\""));
}

#[test]
fn regression_resolve_skill_trust_roots_requires_file_for_mutations() {
    let mut cli = test_cli();
    cli.skill_trust_add = vec!["root=YQ==".to_string()];
    let error = resolve_skill_trust_roots(&cli).expect_err("should fail");
    assert!(error
        .to_string()
        .contains("--skill-trust-root-file is required"));
}

#[test]
fn unit_stream_text_chunks_preserve_whitespace_boundaries() {
    let chunks = stream_text_chunks("hello world\nnext");
    assert_eq!(chunks, vec!["hello ", "world\n", "next"]);
}

#[test]
fn regression_stream_text_chunks_handles_empty_and_single_word() {
    assert!(stream_text_chunks("").is_empty());
    assert_eq!(stream_text_chunks("token"), vec!["token"]);
}

#[test]
fn unit_tool_audit_event_json_for_start_has_expected_shape() {
    let mut starts = HashMap::new();
    let event = AgentEvent::ToolExecutionStart {
        tool_call_id: "call-1".to_string(),
        tool_name: "bash".to_string(),
        arguments: serde_json::json!({ "command": "pwd" }),
    };
    let payload = tool_audit_event_json(&event, &mut starts).expect("expected payload");

    assert_eq!(payload["event"], "tool_execution_start");
    assert_eq!(payload["tool_call_id"], "call-1");
    assert_eq!(payload["tool_name"], "bash");
    assert!(payload["arguments_bytes"].as_u64().unwrap_or(0) > 0);
    assert!(starts.contains_key("call-1"));
}

#[test]
fn unit_tool_audit_event_json_for_end_tracks_duration_and_error_state() {
    let mut starts = HashMap::new();
    starts.insert("call-2".to_string(), Instant::now());
    let event = AgentEvent::ToolExecutionEnd {
        tool_call_id: "call-2".to_string(),
        tool_name: "read".to_string(),
        result: ToolExecutionResult::error(serde_json::json!({ "error": "denied" })),
    };
    let payload = tool_audit_event_json(&event, &mut starts).expect("expected payload");

    assert_eq!(payload["event"], "tool_execution_end");
    assert_eq!(payload["tool_call_id"], "call-2");
    assert_eq!(payload["is_error"], true);
    assert!(payload["result_bytes"].as_u64().unwrap_or(0) > 0);
    assert!(payload["duration_ms"].is_number() || payload["duration_ms"].is_null());
    assert!(!starts.contains_key("call-2"));
}

#[test]
fn integration_tool_audit_logger_persists_jsonl_records() {
    let temp = tempdir().expect("tempdir");
    let log_path = temp.path().join("tool-audit.jsonl");
    let logger = ToolAuditLogger::open(log_path.clone()).expect("logger should open");

    let start = AgentEvent::ToolExecutionStart {
        tool_call_id: "call-3".to_string(),
        tool_name: "write".to_string(),
        arguments: serde_json::json!({ "path": "out.txt", "content": "x" }),
    };
    logger.log_event(&start).expect("write start event");

    let end = AgentEvent::ToolExecutionEnd {
        tool_call_id: "call-3".to_string(),
        tool_name: "write".to_string(),
        result: ToolExecutionResult::ok(serde_json::json!({ "bytes_written": 1 })),
    };
    logger.log_event(&end).expect("write end event");

    let raw = std::fs::read_to_string(log_path).expect("read audit log");
    let lines = raw.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);

    let first: serde_json::Value = serde_json::from_str(lines[0]).expect("parse first");
    let second: serde_json::Value = serde_json::from_str(lines[1]).expect("parse second");
    assert_eq!(first["event"], "tool_execution_start");
    assert_eq!(second["event"], "tool_execution_end");
    assert_eq!(second["is_error"], false);
}

#[test]
fn unit_percentile_duration_ms_handles_empty_and_unsorted_values() {
    assert_eq!(percentile_duration_ms(&[], 50), 0);
    assert_eq!(percentile_duration_ms(&[9], 95), 9);
    assert_eq!(percentile_duration_ms(&[50, 10, 20, 40, 30], 50), 30);
    assert_eq!(percentile_duration_ms(&[50, 10, 20, 40, 30], 95), 50);
}

#[test]
fn functional_summarize_audit_file_aggregates_tool_and_provider_metrics() {
    let temp = tempdir().expect("tempdir");
    let log_path = temp.path().join("audit.jsonl");
    let rows = [
        serde_json::json!({
            "event": "tool_execution_end",
            "tool_name": "bash",
            "duration_ms": 12,
            "is_error": false
        }),
        serde_json::json!({
            "event": "tool_execution_end",
            "tool_name": "bash",
            "duration_ms": 32,
            "is_error": true
        }),
        serde_json::json!({
            "record_type": "prompt_telemetry_v1",
            "provider": "openai",
            "status": "completed",
            "success": true,
            "duration_ms": 100,
            "token_usage": {
                "input_tokens": 4,
                "output_tokens": 2,
                "total_tokens": 6
            }
        }),
        serde_json::json!({
            "record_type": "prompt_telemetry_v1",
            "provider": "openai",
            "status": "interrupted",
            "success": false,
            "duration_ms": 180,
            "token_usage": {
                "input_tokens": 1,
                "output_tokens": 1,
                "total_tokens": 2
            }
        }),
    ]
    .iter()
    .map(serde_json::Value::to_string)
    .collect::<Vec<_>>()
    .join("\n");
    std::fs::write(&log_path, format!("{rows}\n")).expect("write audit log");

    let summary = summarize_audit_file(&log_path).expect("summary");
    assert_eq!(summary.record_count, 4);
    assert_eq!(summary.tool_event_count, 2);
    assert_eq!(summary.prompt_record_count, 2);

    let tool = summary.tools.get("bash").expect("tool aggregate");
    assert_eq!(tool.count, 2);
    assert_eq!(tool.error_count, 1);
    assert_eq!(percentile_duration_ms(&tool.durations_ms, 50), 12);
    assert_eq!(percentile_duration_ms(&tool.durations_ms, 95), 32);

    let provider = summary.providers.get("openai").expect("provider aggregate");
    assert_eq!(provider.count, 2);
    assert_eq!(provider.error_count, 1);
    assert_eq!(provider.input_tokens, 5);
    assert_eq!(provider.output_tokens, 3);
    assert_eq!(provider.total_tokens, 8);
}

#[test]
fn functional_render_audit_summary_includes_expected_sections() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("audit.jsonl");
    std::fs::write(&path, "").expect("write empty log");
    let summary = summarize_audit_file(path.as_path()).expect("empty summary should parse");
    let output = render_audit_summary(&path, &summary);
    assert!(output.contains("audit summary:"));
    assert!(output.contains("tool_breakdown:"));
    assert!(output.contains("provider_breakdown:"));
}

#[test]
fn integration_prompt_telemetry_logger_persists_completed_record() {
    let temp = tempdir().expect("tempdir");
    let log_path = temp.path().join("prompt-telemetry.jsonl");
    let logger = PromptTelemetryLogger::open(log_path.clone(), "openai", "gpt-4o-mini")
        .expect("logger open");

    logger
        .log_event(&AgentEvent::AgentStart)
        .expect("agent start");
    logger
        .log_event(&AgentEvent::TurnEnd {
            turn: 1,
            tool_results: 0,
            request_duration_ms: 44,
            usage: ChatUsage {
                input_tokens: 4,
                output_tokens: 2,
                total_tokens: 6,
            },
            finish_reason: Some("stop".to_string()),
        })
        .expect("turn end");
    logger
        .log_event(&AgentEvent::AgentEnd { new_messages: 2 })
        .expect("agent end");

    let raw = std::fs::read_to_string(log_path).expect("read telemetry log");
    let lines = raw.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 1);
    let record: serde_json::Value = serde_json::from_str(lines[0]).expect("parse record");
    assert_eq!(record["record_type"], "prompt_telemetry_v1");
    assert_eq!(record["provider"], "openai");
    assert_eq!(record["model"], "gpt-4o-mini");
    assert_eq!(record["status"], "completed");
    assert_eq!(record["success"], true);
    assert_eq!(record["finish_reason"], "stop");
    assert_eq!(record["token_usage"]["total_tokens"], 6);
    assert_eq!(record["redaction_policy"]["prompt_content"], "omitted");
}

#[test]
fn regression_prompt_telemetry_logger_marks_interrupted_runs() {
    let temp = tempdir().expect("tempdir");
    let log_path = temp.path().join("prompt-telemetry.jsonl");
    let logger = PromptTelemetryLogger::open(log_path.clone(), "openai", "gpt-4o-mini")
        .expect("logger open");

    logger
        .log_event(&AgentEvent::AgentStart)
        .expect("first start");
    logger
        .log_event(&AgentEvent::TurnEnd {
            turn: 1,
            tool_results: 0,
            request_duration_ms: 11,
            usage: ChatUsage {
                input_tokens: 1,
                output_tokens: 1,
                total_tokens: 2,
            },
            finish_reason: Some("length".to_string()),
        })
        .expect("first turn");
    logger
        .log_event(&AgentEvent::AgentStart)
        .expect("second start");
    logger
        .log_event(&AgentEvent::AgentEnd { new_messages: 1 })
        .expect("finalize second run");

    let raw = std::fs::read_to_string(log_path).expect("read telemetry log");
    let lines = raw.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);

    let first: serde_json::Value = serde_json::from_str(lines[0]).expect("first record");
    let second: serde_json::Value = serde_json::from_str(lines[1]).expect("second record");
    assert_eq!(first["status"], "interrupted");
    assert_eq!(first["success"], false);
    assert_eq!(second["status"], "completed");
    assert_eq!(second["success"], true);
}

#[test]
fn regression_summarize_audit_file_remains_compatible_with_tool_audit_logs() {
    let temp = tempdir().expect("tempdir");
    let log_path = temp.path().join("tool-audit.jsonl");
    let logger = ToolAuditLogger::open(log_path.clone()).expect("logger should open");
    logger
        .log_event(&AgentEvent::ToolExecutionStart {
            tool_call_id: "call-1".to_string(),
            tool_name: "read".to_string(),
            arguments: serde_json::json!({ "path": "README.md" }),
        })
        .expect("start");
    logger
        .log_event(&AgentEvent::ToolExecutionEnd {
            tool_call_id: "call-1".to_string(),
            tool_name: "read".to_string(),
            result: ToolExecutionResult::ok(serde_json::json!({ "ok": true })),
        })
        .expect("end");

    let summary = summarize_audit_file(&log_path).expect("summarize");
    assert_eq!(summary.record_count, 2);
    assert_eq!(summary.tool_event_count, 1);
    assert_eq!(summary.prompt_record_count, 0);
    assert!(summary.providers.is_empty());
}

#[tokio::test]
async fn integration_run_prompt_with_cancellation_completes_when_not_cancelled() {
    let mut agent = Agent::new(Arc::new(SuccessClient), AgentConfig::default());
    let mut runtime = None;

    let status = run_prompt_with_cancellation(
        &mut agent,
        &mut runtime,
        "hello",
        0,
        pending::<()>(),
        test_render_options(),
    )
    .await
    .expect("prompt should complete");

    assert_eq!(status, PromptRunStatus::Completed);
    assert_eq!(agent.messages().len(), 3);
    assert_eq!(agent.messages()[1].role, MessageRole::User);
    assert_eq!(agent.messages()[2].role, MessageRole::Assistant);
}

#[tokio::test]
async fn integration_tool_hook_subscriber_dispatches_pre_and_post_tool_call_hooks() {
    let temp = tempdir().expect("tempdir");
    let read_target = temp.path().join("README.md");
    std::fs::write(&read_target, "hello from test").expect("write read target");

    let extension_root = temp.path().join("extensions");
    let extension_dir = extension_root.join("tool-observer");
    std::fs::create_dir_all(&extension_dir).expect("create extension dir");
    let request_log = extension_dir.join("requests.ndjson");
    let hook_script = extension_dir.join("hook.sh");
    std::fs::write(
        &hook_script,
        format!(
            "#!/usr/bin/env bash\nset -euo pipefail\ninput=\"$(cat)\"\nprintf '%s\\n' \"$input\" >> \"{}\"\nprintf '{{\"ok\":true}}'\n",
            request_log.display()
        ),
    )
    .expect("write hook script");
    make_script_executable(&hook_script);
    std::fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "tool-observer",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "hook.sh",
  "hooks": ["pre-tool-call", "post-tool-call"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write extension manifest");

    let responses = VecDeque::from(vec![
        ChatResponse {
            message: tau_ai::Message::assistant_blocks(vec![ContentBlock::ToolCall {
                id: "call-1".to_string(),
                name: "read".to_string(),
                arguments: serde_json::json!({
                    "path": read_target.display().to_string(),
                }),
            }]),
            finish_reason: Some("tool_calls".to_string()),
            usage: ChatUsage::default(),
        },
        ChatResponse {
            message: Message::assistant_text("tool flow complete"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        },
    ]);

    let mut agent = Agent::new(
        Arc::new(QueueClient {
            responses: AsyncMutex::new(responses),
        }),
        AgentConfig::default(),
    );
    let policy = crate::tools::ToolPolicy::new(vec![temp.path().to_path_buf()]);
    crate::tools::register_builtin_tools(&mut agent, policy);

    let hook_config = RuntimeExtensionHooksConfig {
        enabled: true,
        root: extension_root.clone(),
    };
    register_runtime_extension_tool_hook_subscriber(&mut agent, &hook_config);

    let mut runtime = None;
    let status = run_prompt_with_cancellation(
        &mut agent,
        &mut runtime,
        "read the file",
        0,
        pending::<()>(),
        test_render_options(),
    )
    .await
    .expect("prompt should succeed");
    assert_eq!(status, PromptRunStatus::Completed);

    let raw = std::fs::read_to_string(&request_log).expect("read request log");
    let rows = raw
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("json row"))
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["hook"], "pre-tool-call");
    assert_eq!(rows[1]["hook"], "post-tool-call");
    assert_eq!(rows[0]["payload"]["schema_version"], 1);
    assert_eq!(rows[1]["payload"]["schema_version"], 1);
    assert_eq!(rows[0]["payload"]["hook"], "pre-tool-call");
    assert_eq!(rows[1]["payload"]["hook"], "post-tool-call");
    assert!(rows[0]["payload"]["emitted_at_ms"].as_u64().is_some());
    assert!(rows[1]["payload"]["emitted_at_ms"].as_u64().is_some());
    assert_eq!(rows[0]["payload"]["data"]["tool_name"], "read");
    assert_eq!(rows[1]["payload"]["data"]["tool_name"], "read");
    assert_eq!(rows[1]["payload"]["data"]["result"]["is_error"], false);
}

#[tokio::test]
async fn regression_tool_hook_subscriber_timeout_does_not_fail_prompt() {
    let temp = tempdir().expect("tempdir");
    let read_target = temp.path().join("README.md");
    std::fs::write(&read_target, "hello from timeout test").expect("write read target");

    let extension_root = temp.path().join("extensions");
    let extension_dir = extension_root.join("slow-tool-observer");
    std::fs::create_dir_all(&extension_dir).expect("create extension dir");
    let hook_script = extension_dir.join("hook.sh");
    std::fs::write(
        &hook_script,
        "#!/usr/bin/env bash\nsleep 1\nprintf '{\"ok\":true}'\n",
    )
    .expect("write hook script");
    make_script_executable(&hook_script);
    std::fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "slow-tool-observer",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "hook.sh",
  "hooks": ["pre-tool-call", "post-tool-call"],
  "permissions": ["run-commands"],
  "timeout_ms": 20
}"#,
    )
    .expect("write extension manifest");

    let responses = VecDeque::from(vec![
        ChatResponse {
            message: tau_ai::Message::assistant_blocks(vec![ContentBlock::ToolCall {
                id: "call-1".to_string(),
                name: "read".to_string(),
                arguments: serde_json::json!({
                    "path": read_target.display().to_string(),
                }),
            }]),
            finish_reason: Some("tool_calls".to_string()),
            usage: ChatUsage::default(),
        },
        ChatResponse {
            message: Message::assistant_text("tool flow survived timeout"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        },
    ]);

    let mut agent = Agent::new(
        Arc::new(QueueClient {
            responses: AsyncMutex::new(responses),
        }),
        AgentConfig::default(),
    );
    let policy = crate::tools::ToolPolicy::new(vec![temp.path().to_path_buf()]);
    crate::tools::register_builtin_tools(&mut agent, policy);

    let hook_config = RuntimeExtensionHooksConfig {
        enabled: true,
        root: extension_root,
    };
    register_runtime_extension_tool_hook_subscriber(&mut agent, &hook_config);

    let mut runtime = None;
    let status = run_prompt_with_cancellation(
        &mut agent,
        &mut runtime,
        "read the file",
        0,
        pending::<()>(),
        test_render_options(),
    )
    .await
    .expect("prompt should still succeed when hook times out");
    assert_eq!(status, PromptRunStatus::Completed);
    assert_eq!(
        agent
            .messages()
            .last()
            .expect("assistant response")
            .text_content(),
        "tool flow survived timeout"
    );
}

#[tokio::test]
async fn integration_extension_registered_tool_executes_in_prompt_loop() {
    let temp = tempdir().expect("tempdir");
    let extension_root = temp.path().join("extensions");
    let extension_dir = extension_root.join("tool-registry");
    std::fs::create_dir_all(&extension_dir).expect("create extension dir");
    let request_log = extension_dir.join("tool-request.ndjson");
    let tool_script = extension_dir.join("tool.sh");
    std::fs::write(
        &tool_script,
        format!(
            "#!/bin/sh\nread -r input\nprintf '%s\\n' \"$input\" >> \"{}\"\nprintf '{{\"content\":{{\"status\":\"ok\",\"source\":\"extension\"}},\"is_error\":false}}'\n",
            request_log.display()
        ),
    )
    .expect("write tool script");
    make_script_executable(&tool_script);
    std::fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "tool-registry",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "tool.sh",
  "permissions": ["run-commands"],
  "tools": [
    {
      "name": "issue_triage",
      "description": "triage issue labels",
      "parameters": {
        "type": "object",
        "properties": {
          "title": {"type":"string"}
        },
        "required": ["title"],
        "additionalProperties": false
      }
    }
  ]
}"#,
    )
    .expect("write extension manifest");

    let registrations = discover_extension_runtime_registrations(&extension_root);
    assert_eq!(registrations.registered_tools.len(), 1);

    let responses = VecDeque::from(vec![
        ChatResponse {
            message: tau_ai::Message::assistant_blocks(vec![ContentBlock::ToolCall {
                id: "call-1".to_string(),
                name: "issue_triage".to_string(),
                arguments: serde_json::json!({
                    "title": "bug report",
                }),
            }]),
            finish_reason: Some("tool_calls".to_string()),
            usage: ChatUsage::default(),
        },
        ChatResponse {
            message: Message::assistant_text("extension tool complete"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        },
    ]);

    let mut agent = Agent::new(
        Arc::new(QueueClient {
            responses: AsyncMutex::new(responses),
        }),
        AgentConfig::default(),
    );
    let policy = crate::tools::ToolPolicy::new(vec![temp.path().to_path_buf()]);
    crate::tools::register_builtin_tools(&mut agent, policy);
    register_extension_tools(&mut agent, &registrations.registered_tools);

    let mut runtime = None;
    let status = run_prompt_with_cancellation(
        &mut agent,
        &mut runtime,
        "run extension tool",
        0,
        pending::<()>(),
        test_render_options(),
    )
    .await
    .expect("prompt should succeed");
    assert_eq!(status, PromptRunStatus::Completed);

    let raw = std::fs::read_to_string(&request_log).expect("read request log");
    let payload: serde_json::Value =
        serde_json::from_str(raw.lines().next().expect("one row")).expect("request row json");
    assert_eq!(payload["hook"], "tool-call");
    assert_eq!(payload["payload"]["kind"], "tool-call");
    assert_eq!(payload["payload"]["tool"]["name"], "issue_triage");
    assert_eq!(
        payload["payload"]["tool"]["arguments"]["title"],
        "bug report"
    );
    assert!(agent.messages().iter().any(|message| {
        message.role == MessageRole::Tool
            && message.text_content().contains("\"status\": \"ok\"")
            && message.text_content().contains("\"source\": \"extension\"")
    }));
}

#[test]
fn integration_handle_command_dispatches_extension_registered_command() {
    let temp = tempdir().expect("tempdir");
    let extension_root = temp.path().join("extensions");
    let extension_dir = extension_root.join("command-registry");
    std::fs::create_dir_all(&extension_dir).expect("create extension dir");
    let request_log = extension_dir.join("command-request.ndjson");
    let command_script = extension_dir.join("command.sh");
    std::fs::write(
        &command_script,
        format!(
            "#!/bin/sh\nread -r input\nprintf '%s\\n' \"$input\" >> \"{}\"\nprintf '{{\"output\":\"triage complete\",\"action\":\"continue\"}}'\n",
            request_log.display()
        ),
    )
    .expect("write command script");
    make_script_executable(&command_script);
    std::fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "command-registry",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "command.sh",
  "permissions": ["run-commands"],
  "commands": [
    {
      "name": "/triage-now",
      "description": "run triage command"
    }
  ]
}"#,
    )
    .expect("write extension manifest");
    let registrations = discover_extension_runtime_registrations(&extension_root);
    assert_eq!(registrations.registered_commands.len(), 1);

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let mut runtime = None;
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let auth_command_config = test_auth_command_config();
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("create skills dir");
    let lock_path = default_skills_lock_path(&skills_dir);
    std::fs::write(&lock_path, "{}\n").expect("write lock path");
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

    let action = handle_command_with_session_import_mode(
        "/triage-now 42",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &auth_command_config,
        &ModelCatalog::built_in(),
        &registrations.registered_commands,
    )
    .expect("command should execute");
    assert_eq!(action, CommandAction::Continue);

    let raw = std::fs::read_to_string(&request_log).expect("read request log");
    let payload: serde_json::Value =
        serde_json::from_str(raw.lines().next().expect("one row")).expect("request row json");
    assert_eq!(payload["hook"], "command-call");
    assert_eq!(payload["payload"]["kind"], "command-call");
    assert_eq!(payload["payload"]["command"]["name"], "/triage-now");
    assert_eq!(payload["payload"]["command"]["args"], "42");
}

#[test]
fn regression_handle_command_extension_failure_is_fail_isolated() {
    let temp = tempdir().expect("tempdir");
    let extension_root = temp.path().join("extensions");
    let extension_dir = extension_root.join("command-registry");
    std::fs::create_dir_all(&extension_dir).expect("create extension dir");
    let command_script = extension_dir.join("command.sh");
    std::fs::write(
        &command_script,
        "#!/bin/sh\nread -r _input\nprintf '{\"action\":123}'\n",
    )
    .expect("write command script");
    make_script_executable(&command_script);
    std::fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "command-registry",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "command.sh",
  "permissions": ["run-commands"],
  "commands": [
    {
      "name": "/triage-now",
      "description": "run triage command"
    }
  ]
}"#,
    )
    .expect("write extension manifest");
    let registrations = discover_extension_runtime_registrations(&extension_root);
    assert_eq!(registrations.registered_commands.len(), 1);

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let mut runtime = None;
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let auth_command_config = test_auth_command_config();
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("create skills dir");
    let lock_path = default_skills_lock_path(&skills_dir);
    std::fs::write(&lock_path, "{}\n").expect("write lock path");
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

    let action = handle_command_with_session_import_mode(
        "/triage-now 42",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &auth_command_config,
        &ModelCatalog::built_in(),
        &registrations.registered_commands,
    )
    .expect("errors should be fail-isolated");
    assert_eq!(action, CommandAction::Continue);
}

#[test]
fn unit_parse_numbered_plan_steps_accepts_deterministic_step_format() {
    let steps = parse_numbered_plan_steps("1. Gather context\n2) Implement fix\n3. Verify");
    assert_eq!(
        steps,
        vec![
            "Gather context".to_string(),
            "Implement fix".to_string(),
            "Verify".to_string(),
        ]
    );
}

#[tokio::test]
async fn functional_run_plan_first_prompt_executes_planner_then_executor() {
    let planner_response = ChatResponse {
        message: Message::assistant_text("1. Inspect constraints\n2. Apply change"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let executor_response = ChatResponse {
        message: Message::assistant_text("final implementation response"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let mut agent = Agent::new(
        Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([
                Ok(planner_response),
                Ok(executor_response),
            ])),
        }),
        AgentConfig::default(),
    );
    let mut runtime = None;

    run_plan_first_prompt(
        &mut agent,
        &mut runtime,
        "ship feature",
        0,
        test_render_options(),
        4,
        4,
        512,
        512,
        2_048,
        false,
    )
    .await
    .expect("plan-first prompt should succeed");

    assert_eq!(agent.messages().len(), 5);
    assert_eq!(
        agent
            .messages()
            .last()
            .expect("assistant response")
            .text_content(),
        "final implementation response"
    );
}

#[tokio::test]
async fn functional_run_plan_first_prompt_delegate_steps_executes_and_consolidates() {
    let planner_response = ChatResponse {
        message: Message::assistant_text("1. Inspect constraints\n2. Apply change"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let delegated_step_one = ChatResponse {
        message: Message::assistant_text("constraints reviewed"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let delegated_step_two = ChatResponse {
        message: Message::assistant_text("change applied"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let consolidation_response = ChatResponse {
        message: Message::assistant_text("final delegated response"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let mut agent = Agent::new(
        Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([
                Ok(planner_response),
                Ok(delegated_step_one),
                Ok(delegated_step_two),
                Ok(consolidation_response),
            ])),
        }),
        AgentConfig::default(),
    );
    let mut runtime = None;

    run_plan_first_prompt(
        &mut agent,
        &mut runtime,
        "ship feature",
        0,
        test_render_options(),
        4,
        4,
        512,
        512,
        1_024,
        true,
    )
    .await
    .expect("delegated plan-first prompt should succeed");

    let messages = agent.messages();
    assert_eq!(
        messages.last().expect("assistant response").text_content(),
        "final delegated response"
    );
    assert!(messages
        .iter()
        .any(|message| message.text_content() == "constraints reviewed"));
    assert!(messages
        .iter()
        .any(|message| message.text_content() == "change applied"));
}

#[tokio::test]
async fn regression_run_plan_first_prompt_with_policy_context_fails_when_context_missing() {
    let planner_response = ChatResponse {
        message: Message::assistant_text("1. Inspect constraints\n2. Apply change"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let delegated_unused_response = ChatResponse {
        message: Message::assistant_text("should not execute"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let mut agent = Agent::new(
        Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([
                Ok(planner_response),
                Ok(delegated_unused_response),
            ])),
        }),
        AgentConfig::default(),
    );
    let mut runtime = None;

    let error = run_plan_first_prompt_with_policy_context(
        &mut agent,
        &mut runtime,
        "ship feature",
        0,
        test_render_options(),
        4,
        4,
        512,
        512,
        1_024,
        true,
        None,
    )
    .await
    .expect_err("missing delegated policy context should fail closed");
    assert!(error
        .to_string()
        .contains("delegated policy inheritance context is unavailable"));
    assert!(!agent
        .messages()
        .iter()
        .any(|message| message.text_content() == "should not execute"));
}

#[tokio::test]
async fn regression_run_plan_first_prompt_delegate_steps_fails_on_empty_step_output() {
    let planner_response = ChatResponse {
        message: Message::assistant_text("1. Inspect constraints\n2. Apply change"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let delegated_empty_response = ChatResponse {
        message: Message::assistant_text(""),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let delegated_unused_response = ChatResponse {
        message: Message::assistant_text("should not execute"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let mut agent = Agent::new(
        Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([
                Ok(planner_response),
                Ok(delegated_empty_response),
                Ok(delegated_unused_response),
            ])),
        }),
        AgentConfig::default(),
    );
    let mut runtime = None;

    let error = run_plan_first_prompt(
        &mut agent,
        &mut runtime,
        "ship feature",
        0,
        test_render_options(),
        4,
        4,
        512,
        512,
        1_024,
        true,
    )
    .await
    .expect_err("empty delegated output should fail");
    assert!(error
        .to_string()
        .contains("delegated step 1 produced no text output"));
    assert!(!agent
        .messages()
        .iter()
        .any(|message| message.text_content() == "should not execute"));
}

#[tokio::test]
async fn regression_run_plan_first_prompt_delegate_steps_fails_when_step_count_exceeds_budget() {
    let planner_response = ChatResponse {
        message: Message::assistant_text(
            "1. Inspect constraints\n2. Apply change\n3. Verify output",
        ),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let delegated_unused_response = ChatResponse {
        message: Message::assistant_text("should not execute"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let mut agent = Agent::new(
        Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([
                Ok(planner_response),
                Ok(delegated_unused_response),
            ])),
        }),
        AgentConfig::default(),
    );
    let mut runtime = None;

    let error = run_plan_first_prompt(
        &mut agent,
        &mut runtime,
        "ship feature",
        0,
        test_render_options(),
        4,
        2,
        512,
        512,
        1_024,
        true,
    )
    .await
    .expect_err("delegated step count above budget should fail");
    assert!(error.to_string().contains("delegated step budget exceeded"));
    assert!(!agent
        .messages()
        .iter()
        .any(|message| message.text_content() == "should not execute"));
}

#[tokio::test]
async fn regression_run_plan_first_prompt_delegate_steps_fails_when_step_output_exceeds_budget() {
    let planner_response = ChatResponse {
        message: Message::assistant_text("1. Inspect constraints\n2. Apply change"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let delegated_over_budget_response = ChatResponse {
        message: Message::assistant_text("over budget delegated output"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let delegated_unused_response = ChatResponse {
        message: Message::assistant_text("should not execute"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let mut agent = Agent::new(
        Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([
                Ok(planner_response),
                Ok(delegated_over_budget_response),
                Ok(delegated_unused_response),
            ])),
        }),
        AgentConfig::default(),
    );
    let mut runtime = None;

    let error = run_plan_first_prompt(
        &mut agent,
        &mut runtime,
        "ship feature",
        0,
        test_render_options(),
        4,
        4,
        512,
        8,
        1_024,
        true,
    )
    .await
    .expect_err("oversized delegated output should fail");
    assert!(error
        .to_string()
        .contains("delegated step 1 response exceeded budget"));
    assert!(!agent
        .messages()
        .iter()
        .any(|message| message.text_content() == "should not execute"));
}

#[tokio::test]
async fn regression_run_plan_first_prompt_delegate_steps_fails_when_total_output_exceeds_budget() {
    let planner_response = ChatResponse {
        message: Message::assistant_text("1. Inspect constraints\n2. Apply change"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let delegated_step_one = ChatResponse {
        message: Message::assistant_text("step one"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let delegated_step_two = ChatResponse {
        message: Message::assistant_text("step two"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let consolidation_unused_response = ChatResponse {
        message: Message::assistant_text("should not execute"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let mut agent = Agent::new(
        Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([
                Ok(planner_response),
                Ok(delegated_step_one),
                Ok(delegated_step_two),
                Ok(consolidation_unused_response),
            ])),
        }),
        AgentConfig::default(),
    );
    let mut runtime = None;

    let error = run_plan_first_prompt(
        &mut agent,
        &mut runtime,
        "ship feature",
        0,
        test_render_options(),
        4,
        4,
        512,
        64,
        12,
        true,
    )
    .await
    .expect_err("oversized delegated cumulative output should fail");
    assert!(error
        .to_string()
        .contains("delegated responses exceeded cumulative budget"));
    assert!(!agent
        .messages()
        .iter()
        .any(|message| message.text_content() == "should not execute"));
}

#[tokio::test]
async fn regression_run_plan_first_prompt_rejects_overlong_plans_before_executor_phase() {
    let planner_response = ChatResponse {
        message: Message::assistant_text("1. Step one\n2. Step two\n3. Step three"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let executor_response = ChatResponse {
        message: Message::assistant_text("should not execute"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let mut agent = Agent::new(
        Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([
                Ok(planner_response),
                Ok(executor_response),
            ])),
        }),
        AgentConfig::default(),
    );
    let mut runtime = None;

    let error = run_plan_first_prompt(
        &mut agent,
        &mut runtime,
        "ship feature",
        0,
        test_render_options(),
        2,
        2,
        512,
        512,
        1_024,
        false,
    )
    .await
    .expect_err("overlong plan should fail");
    assert!(error.to_string().contains("planner produced 3 steps"));
    assert!(!agent
        .messages()
        .iter()
        .any(|message| message.text_content() == "should not execute"));
}

#[tokio::test]
async fn regression_run_plan_first_prompt_fails_when_executor_output_is_empty() {
    let planner_response = ChatResponse {
        message: Message::assistant_text("1. Inspect constraints\n2. Apply change"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let executor_response = ChatResponse {
        message: Message::assistant_text(""),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let mut agent = Agent::new(
        Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([
                Ok(planner_response),
                Ok(executor_response),
            ])),
        }),
        AgentConfig::default(),
    );
    let mut runtime = None;

    let error = run_plan_first_prompt(
        &mut agent,
        &mut runtime,
        "ship feature",
        0,
        test_render_options(),
        4,
        4,
        512,
        512,
        1_024,
        false,
    )
    .await
    .expect_err("empty executor output should fail");
    assert!(error
        .to_string()
        .contains("executor produced no text output"));
}

#[tokio::test]
async fn regression_run_plan_first_prompt_fails_when_executor_output_exceeds_budget() {
    let planner_response = ChatResponse {
        message: Message::assistant_text("1. Inspect constraints\n2. Apply change"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let executor_response = ChatResponse {
        message: Message::assistant_text("final implementation response"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let mut agent = Agent::new(
        Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([
                Ok(planner_response),
                Ok(executor_response),
            ])),
        }),
        AgentConfig::default(),
    );
    let mut runtime = None;

    let error = run_plan_first_prompt(
        &mut agent,
        &mut runtime,
        "ship feature",
        0,
        test_render_options(),
        4,
        4,
        8,
        512,
        1_024,
        false,
    )
    .await
    .expect_err("oversized executor output should fail");
    assert!(error
        .to_string()
        .contains("executor response exceeded budget"));
}

#[tokio::test]
async fn regression_run_prompt_with_cancellation_restores_agent_state() {
    let mut agent = Agent::new(Arc::new(SlowClient), AgentConfig::default());
    let initial_messages = agent.messages().to_vec();
    let mut runtime = None;

    let status = run_prompt_with_cancellation(
        &mut agent,
        &mut runtime,
        "cancel me",
        0,
        ready(()),
        test_render_options(),
    )
    .await
    .expect("cancellation branch should succeed");

    assert_eq!(status, PromptRunStatus::Cancelled);
    assert_eq!(agent.messages().len(), initial_messages.len());
    assert_eq!(agent.messages()[0].role, initial_messages[0].role);
    assert_eq!(
        agent.messages()[0].text_content(),
        initial_messages[0].text_content()
    );
}

#[tokio::test]
async fn functional_run_prompt_with_timeout_restores_agent_state() {
    let mut agent = Agent::new(Arc::new(SlowClient), AgentConfig::default());
    let initial_messages = agent.messages().to_vec();
    let mut runtime = None;

    let status = run_prompt_with_cancellation(
        &mut agent,
        &mut runtime,
        "timeout me",
        20,
        pending::<()>(),
        test_render_options(),
    )
    .await
    .expect("timeout branch should succeed");

    assert_eq!(status, PromptRunStatus::TimedOut);
    assert_eq!(agent.messages().len(), initial_messages.len());
    assert_eq!(
        agent.messages()[0].text_content(),
        initial_messages[0].text_content()
    );
}

#[tokio::test]
async fn integration_regression_cancellation_does_not_persist_partial_session_entries() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("cancel-session.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    let active_head = store
        .ensure_initialized("You are a helpful coding assistant.")
        .expect("initialize session");

    let mut runtime = Some(SessionRuntime { store, active_head });
    let mut agent = Agent::new(Arc::new(SlowClient), AgentConfig::default());

    let status = run_prompt_with_cancellation(
        &mut agent,
        &mut runtime,
        "cancel me",
        0,
        ready(()),
        test_render_options(),
    )
    .await
    .expect("cancelled prompt should succeed");

    assert_eq!(status, PromptRunStatus::Cancelled);
    assert_eq!(runtime.as_ref().expect("runtime").store.entries().len(), 1);

    let reloaded = SessionStore::load(&path).expect("reload");
    assert_eq!(reloaded.entries().len(), 1);
}

#[tokio::test]
async fn integration_regression_timeout_does_not_persist_partial_session_entries() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("timeout-session.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    let active_head = store
        .ensure_initialized("You are a helpful coding assistant.")
        .expect("initialize session");

    let mut runtime = Some(SessionRuntime { store, active_head });
    let mut agent = Agent::new(Arc::new(SlowClient), AgentConfig::default());

    let status = run_prompt_with_cancellation(
        &mut agent,
        &mut runtime,
        "timeout me",
        20,
        pending::<()>(),
        test_render_options(),
    )
    .await
    .expect("timed-out prompt should succeed");

    assert_eq!(status, PromptRunStatus::TimedOut);
    assert_eq!(runtime.as_ref().expect("runtime").store.entries().len(), 1);

    let reloaded = SessionStore::load(&path).expect("reload");
    assert_eq!(reloaded.entries().len(), 1);
}

#[tokio::test]
async fn integration_agent_bash_policy_blocks_overlong_commands() {
    let temp = tempdir().expect("tempdir");
    let responses = VecDeque::from(vec![
        ChatResponse {
            message: tau_ai::Message::assistant_blocks(vec![ContentBlock::ToolCall {
                id: "call-1".to_string(),
                name: "bash".to_string(),
                arguments: serde_json::json!({
                    "command": "printf",
                    "cwd": temp.path().display().to_string(),
                }),
            }]),
            finish_reason: Some("tool_calls".to_string()),
            usage: ChatUsage::default(),
        },
        ChatResponse {
            message: tau_ai::Message::assistant_text("done"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        },
    ]);

    let client = Arc::new(QueueClient {
        responses: AsyncMutex::new(responses),
    });
    let mut agent = Agent::new(client, AgentConfig::default());

    let mut policy = crate::tools::ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.max_command_length = 4;
    crate::tools::register_builtin_tools(&mut agent, policy);

    let new_messages = agent
        .prompt("run command")
        .await
        .expect("prompt should succeed");
    let tool_message = new_messages
        .iter()
        .find(|message| message.role == MessageRole::Tool)
        .expect("tool result should be present");

    assert!(tool_message.is_error);
    assert!(tool_message.text_content().contains("command is too long"));
}

#[tokio::test]
async fn integration_agent_write_policy_blocks_oversized_content() {
    let temp = tempdir().expect("tempdir");
    let target = temp.path().join("target.txt");
    let responses = VecDeque::from(vec![
        ChatResponse {
            message: tau_ai::Message::assistant_blocks(vec![ContentBlock::ToolCall {
                id: "call-1".to_string(),
                name: "write".to_string(),
                arguments: serde_json::json!({
                    "path": target,
                    "content": "hello",
                }),
            }]),
            finish_reason: Some("tool_calls".to_string()),
            usage: ChatUsage::default(),
        },
        ChatResponse {
            message: tau_ai::Message::assistant_text("done"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        },
    ]);

    let client = Arc::new(QueueClient {
        responses: AsyncMutex::new(responses),
    });
    let mut agent = Agent::new(client, AgentConfig::default());

    let mut policy = crate::tools::ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.max_file_write_bytes = 4;
    crate::tools::register_builtin_tools(&mut agent, policy);

    let new_messages = agent
        .prompt("write file")
        .await
        .expect("prompt should succeed");
    let tool_message = new_messages
        .iter()
        .find(|message| message.role == MessageRole::Tool)
        .expect("tool result should be present");

    assert!(tool_message.is_error);
    assert!(tool_message.text_content().contains("content is too large"));
}

#[test]
fn branch_and_resume_commands_reload_agent_messages() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("session.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    let head = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append");
    let head = store
        .append_messages(
            head,
            &[
                tau_ai::Message::user("q1"),
                tau_ai::Message::assistant_text("a1"),
                tau_ai::Message::user("q2"),
                tau_ai::Message::assistant_text("a2"),
            ],
        )
        .expect("append")
        .expect("head id");

    let branch_target = head - 2;

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = store
        .lineage_messages(Some(head))
        .expect("lineage should resolve");
    agent.replace_messages(lineage);

    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let tool_policy_json = test_tool_policy_json();

    let action = handle_command(
        &format!("  /branch    {branch_target}   "),
        &mut agent,
        &mut runtime,
        &tool_policy_json,
    )
    .expect("branch command should succeed");
    assert_eq!(action, CommandAction::Continue);
    assert_eq!(
        runtime.as_ref().and_then(|runtime| runtime.active_head),
        Some(branch_target)
    );
    assert_eq!(agent.messages().len(), 3);

    let action = handle_command("/resume", &mut agent, &mut runtime, &tool_policy_json)
        .expect("resume command should succeed");
    assert_eq!(action, CommandAction::Continue);
    assert_eq!(
        runtime.as_ref().and_then(|runtime| runtime.active_head),
        Some(head)
    );
    assert_eq!(agent.messages().len(), 5);
}

#[test]
fn exit_commands_return_exit_action() {
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let mut runtime = None;
    let tool_policy_json = test_tool_policy_json();

    assert_eq!(
        handle_command("/quit", &mut agent, &mut runtime, &tool_policy_json)
            .expect("quit should succeed"),
        CommandAction::Exit
    );
    assert_eq!(
        handle_command("/exit", &mut agent, &mut runtime, &tool_policy_json)
            .expect("exit should succeed"),
        CommandAction::Exit
    );
}

#[test]
fn policy_command_returns_continue_action() {
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let mut runtime = None;
    let tool_policy_json = test_tool_policy_json();

    let action = handle_command("/policy", &mut agent, &mut runtime, &tool_policy_json)
        .expect("policy should succeed");
    assert_eq!(action, CommandAction::Continue);
}

#[test]
fn functional_session_export_command_writes_active_lineage_snapshot() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session.jsonl");
    let export_path = temp.path().join("snapshot.jsonl");

    let mut store = SessionStore::load(&session_path).expect("load");
    let head = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append");
    let head = store
        .append_messages(
            head,
            &[
                tau_ai::Message::user("q1"),
                tau_ai::Message::assistant_text("a1"),
            ],
        )
        .expect("append")
        .expect("head");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let tool_policy_json = test_tool_policy_json();

    let action = handle_command(
        &format!("/session-export {}", export_path.display()),
        &mut agent,
        &mut runtime,
        &tool_policy_json,
    )
    .expect("session export should succeed");
    assert_eq!(action, CommandAction::Continue);

    let exported = SessionStore::load(&export_path).expect("load exported");
    assert_eq!(exported.entries().len(), 3);
    assert_eq!(exported.entries()[0].message.text_content(), "sys");
    assert_eq!(exported.entries()[1].message.text_content(), "q1");
    assert_eq!(exported.entries()[2].message.text_content(), "a1");
}

#[test]
fn functional_session_import_command_merges_snapshot_and_updates_active_head() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session.jsonl");
    let import_path = temp.path().join("import.jsonl");

    let mut target_store = SessionStore::load(&session_path).expect("load target");
    let target_head = target_store
        .append_messages(None, &[tau_ai::Message::system("target-root")])
        .expect("append target root")
        .expect("target head");
    target_store
        .append_messages(Some(target_head), &[tau_ai::Message::user("target-user")])
        .expect("append target user");

    let mut import_store = SessionStore::load(&import_path).expect("load import");
    let import_head = import_store
        .append_messages(None, &[tau_ai::Message::system("import-root")])
        .expect("append import root");
    import_store
        .append_messages(import_head, &[tau_ai::Message::user("import-user")])
        .expect("append import user");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let target_lineage = target_store
        .lineage_messages(target_store.head_id())
        .expect("target lineage");
    agent.replace_messages(target_lineage);

    let mut runtime = Some(SessionRuntime {
        store: target_store,
        active_head: Some(2),
    });
    let tool_policy_json = test_tool_policy_json();

    let action = handle_command(
        &format!("/session-import {}", import_path.display()),
        &mut agent,
        &mut runtime,
        &tool_policy_json,
    )
    .expect("session import should succeed");
    assert_eq!(action, CommandAction::Continue);

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.store.entries().len(), 4);
    assert_eq!(runtime.active_head, Some(4));
    assert_eq!(runtime.store.entries()[2].id, 3);
    assert_eq!(runtime.store.entries()[2].parent_id, None);
    assert_eq!(runtime.store.entries()[3].id, 4);
    assert_eq!(runtime.store.entries()[3].parent_id, Some(3));
    assert_eq!(agent.messages().len(), 2);
    assert_eq!(agent.messages()[0].text_content(), "import-root");
    assert_eq!(agent.messages()[1].text_content(), "import-user");
}

#[test]
fn integration_session_import_command_replace_mode_overwrites_runtime_state() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session-replace.jsonl");
    let import_path = temp.path().join("import-replace.jsonl");

    let mut target_store = SessionStore::load(&session_path).expect("load target");
    let head = target_store
        .append_messages(None, &[tau_ai::Message::system("target-root")])
        .expect("append target root");
    target_store
        .append_messages(head, &[tau_ai::Message::user("target-user")])
        .expect("append target user");

    let import_raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":10,"parent_id":null,"message":tau_ai::Message::system("import-root")}).to_string(),
            serde_json::json!({"record_type":"entry","id":11,"parent_id":10,"message":tau_ai::Message::assistant_text("import-assistant")}).to_string(),
        ]
        .join("\n");
    std::fs::write(&import_path, format!("{import_raw}\n")).expect("write import snapshot");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let target_lineage = target_store
        .lineage_messages(target_store.head_id())
        .expect("target lineage");
    agent.replace_messages(target_lineage);

    let mut runtime = Some(SessionRuntime {
        store: target_store,
        active_head: Some(2),
    });
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let skills_dir = PathBuf::from(".tau/skills");
    let skills_lock_path = default_skills_lock_path(&skills_dir);
    let skills_command_config = skills_command_config(&skills_dir, &skills_lock_path, None);

    let action = handle_command_with_session_import_mode(
        &format!("/session-import {}", import_path.display()),
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Replace,
        &profile_defaults,
        &skills_command_config,
        &test_auth_command_config(),
        &ModelCatalog::built_in(),
        &[],
    )
    .expect("session replace import should succeed");
    assert_eq!(action, CommandAction::Continue);

    let mut runtime = runtime.expect("runtime");
    assert_eq!(runtime.store.entries().len(), 2);
    assert_eq!(runtime.store.entries()[0].id, 10);
    assert_eq!(runtime.store.entries()[1].id, 11);
    assert_eq!(runtime.active_head, Some(11));
    assert_eq!(agent.messages().len(), 2);
    assert_eq!(agent.messages()[0].text_content(), "import-root");
    assert_eq!(agent.messages()[1].text_content(), "import-assistant");

    let next = runtime
        .store
        .append_messages(
            runtime.active_head,
            &[tau_ai::Message::user("after-replace")],
        )
        .expect("append after replace");
    assert_eq!(next, Some(12));
}

#[test]
fn regression_session_import_command_rejects_invalid_snapshot() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session-invalid.jsonl");
    let import_path = temp.path().join("import-invalid.jsonl");

    let mut target_store = SessionStore::load(&session_path).expect("load target");
    target_store
        .append_messages(None, &[tau_ai::Message::system("target-root")])
        .expect("append target");
    let import_raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":1,"parent_id":2,"message":tau_ai::Message::system("cycle-a")}).to_string(),
            serde_json::json!({"record_type":"entry","id":2,"parent_id":1,"message":tau_ai::Message::user("cycle-b")}).to_string(),
        ]
        .join("\n");
    std::fs::write(&import_path, format!("{import_raw}\n")).expect("write invalid import");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let target_lineage = target_store
        .lineage_messages(target_store.head_id())
        .expect("target lineage");
    agent.replace_messages(target_lineage.clone());

    let mut runtime = Some(SessionRuntime {
        store: target_store,
        active_head: Some(1),
    });
    let tool_policy_json = test_tool_policy_json();

    let error = handle_command(
        &format!("/session-import {}", import_path.display()),
        &mut agent,
        &mut runtime,
        &tool_policy_json,
    )
    .expect_err("invalid import should fail");
    assert!(error
        .to_string()
        .contains("import session validation failed"));

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.store.entries().len(), 1);
    assert_eq!(runtime.active_head, Some(1));
    assert_eq!(agent.messages().len(), target_lineage.len());
    assert_eq!(agent.messages()[0].text_content(), "target-root");
}

#[test]
fn functional_validate_session_file_succeeds_for_valid_session() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session.jsonl");

    let mut store = SessionStore::load(&session_path).expect("load");
    let head = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append");
    store
        .append_messages(head, &[tau_ai::Message::user("hello")])
        .expect("append");

    let mut cli = test_cli();
    cli.session = session_path;
    cli.session_validate = true;

    validate_session_file(&cli).expect("session validation should pass");
}

#[test]
fn regression_validate_session_file_fails_for_invalid_session_graph() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session.jsonl");

    let raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":1,"parent_id":2,"message":tau_ai::Message::system("sys")}).to_string(),
            serde_json::json!({"record_type":"entry","id":2,"parent_id":1,"message":tau_ai::Message::user("cycle")}).to_string(),
        ]
        .join("\n");
    std::fs::write(&session_path, format!("{raw}\n")).expect("write invalid session");

    let mut cli = test_cli();
    cli.session = session_path;
    cli.session_validate = true;

    let error = validate_session_file(&cli).expect_err("session validation should fail for cycle");
    assert!(error.to_string().contains("session validation failed"));
    assert!(error.to_string().contains("cycles=2"));
}

#[test]
fn regression_validate_session_file_rejects_no_session_flag() {
    let mut cli = test_cli();
    cli.no_session = true;
    cli.session_validate = true;

    let error =
        validate_session_file(&cli).expect_err("validation with no-session flag should fail fast");
    assert!(error
        .to_string()
        .contains("--session-validate cannot be used together with --no-session"));
}

#[test]
fn integration_execute_startup_preflight_runs_onboarding_and_generates_report() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.onboard = true;
    cli.onboard_non_interactive = true;
    cli.onboard_profile = "team_default".to_string();

    let handled = execute_startup_preflight(&cli).expect("onboarding preflight");
    assert!(handled);

    let profile_store = temp.path().join(".tau/profiles.json");
    assert!(profile_store.exists(), "profile store should be created");

    let reports_dir = temp.path().join(".tau/reports");
    let reports = std::fs::read_dir(&reports_dir)
        .expect("reports dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("onboarding-") && name.ends_with(".json"))
        })
        .collect::<Vec<_>>();
    assert!(
        !reports.is_empty(),
        "expected at least one onboarding report in {}",
        reports_dir.display()
    );
}

#[test]
fn session_repair_command_runs_successfully() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("session.jsonl");
    let mut store = SessionStore::load(&path).expect("load");
    let head = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append");
    store
        .append_messages(head, &[tau_ai::Message::user("hello")])
        .expect("append");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = store
        .lineage_messages(store.head_id())
        .expect("lineage should resolve");
    agent.replace_messages(lineage);

    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(2),
    });
    let tool_policy_json = test_tool_policy_json();

    let action = handle_command(
        "/session-repair",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
    )
    .expect("repair command should succeed");
    assert_eq!(action, CommandAction::Continue);
    assert_eq!(agent.messages().len(), 2);
}

#[test]
fn session_compact_command_prunes_inactive_branch() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("session-compact.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append")
        .expect("root");
    let head = store
        .append_messages(
            Some(root),
            &[
                tau_ai::Message::user("main-q"),
                tau_ai::Message::assistant_text("main-a"),
            ],
        )
        .expect("append")
        .expect("main head");
    store
        .append_messages(Some(root), &[tau_ai::Message::user("branch-q")])
        .expect("append branch");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = store
        .lineage_messages(Some(head))
        .expect("lineage should resolve");
    agent.replace_messages(lineage);

    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let tool_policy_json = test_tool_policy_json();

    let action = handle_command(
        "/session-compact",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
    )
    .expect("compact command should succeed");
    assert_eq!(action, CommandAction::Continue);

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.store.entries().len(), 3);
    assert_eq!(runtime.store.branch_tips().len(), 1);
    assert_eq!(runtime.store.branch_tips()[0].id, head);
    assert_eq!(agent.messages().len(), 3);
}

#[test]
fn integration_initialize_session_applies_lock_timeout_policy() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("locked-session.jsonl");
    let lock_path = session_path.with_extension("lock");
    std::fs::write(&lock_path, "locked").expect("write lock");

    let mut cli = test_cli();
    cli.session = session_path;
    cli.session_lock_wait_ms = 120;
    cli.session_lock_stale_ms = 0;
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let start = Instant::now();

    let error = initialize_session(&mut agent, &cli, "sys")
        .expect_err("initialization should fail when lock persists");
    assert!(error.to_string().contains("timed out acquiring lock"));
    assert!(start.elapsed() < Duration::from_secs(2));

    std::fs::remove_file(lock_path).expect("cleanup lock");
}

#[test]
fn functional_initialize_session_reclaims_stale_lock_when_enabled() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("stale-lock-session.jsonl");
    let lock_path = session_path.with_extension("lock");
    std::fs::write(&lock_path, "stale").expect("write lock");
    std::thread::sleep(Duration::from_millis(30));

    let mut cli = test_cli();
    cli.session = session_path;
    cli.session_lock_wait_ms = 1_000;
    cli.session_lock_stale_ms = 10;
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());

    let runtime = initialize_session(&mut agent, &cli, "sys")
        .expect("initialization should reclaim stale lock");
    assert_eq!(runtime.store.entries().len(), 1);
    assert!(!lock_path.exists());
}

#[test]
fn unit_parse_sandbox_command_tokens_supports_shell_words_and_placeholders() {
    let tokens = parse_sandbox_command_tokens(&[
        "bwrap".to_string(),
        "--bind".to_string(),
        "\"{cwd}\"".to_string(),
        "{cwd}".to_string(),
        "{shell}".to_string(),
        "{command}".to_string(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        tokens,
        vec![
            "bwrap".to_string(),
            "--bind".to_string(),
            "{cwd}".to_string(),
            "{cwd}".to_string(),
            "{shell}".to_string(),
            "{command}".to_string(),
        ]
    );
}

#[test]
fn regression_parse_sandbox_command_tokens_rejects_invalid_quotes() {
    let error = parse_sandbox_command_tokens(&["\"unterminated".to_string()])
        .expect_err("parse should fail");
    assert!(error
        .to_string()
        .contains("invalid --os-sandbox-command token"));
}

#[test]
fn build_tool_policy_includes_cwd_and_custom_root() {
    let mut cli = test_cli();
    cli.allow_path = vec![PathBuf::from("/tmp")];

    let policy = build_tool_policy(&cli).expect("policy should build");
    assert!(policy.allowed_roots.len() >= 2);
    assert_eq!(policy.bash_timeout_ms, 500);
    assert_eq!(policy.max_command_output_bytes, 1024);
    assert_eq!(policy.max_file_read_bytes, 2048);
    assert_eq!(policy.max_file_write_bytes, 2048);
    assert_eq!(policy.max_command_length, 4096);
    assert!(policy.allow_command_newlines);
    assert_eq!(policy.os_sandbox_mode, OsSandboxMode::Off);
    assert!(policy.os_sandbox_command.is_empty());
    assert!(policy.enforce_regular_files);
    assert_eq!(policy.policy_preset, ToolPolicyPreset::Balanced);
    assert!(!policy.bash_dry_run);
    assert!(!policy.tool_policy_trace);
    assert!(policy.extension_policy_override_root.is_none());
}

#[test]
fn unit_tool_policy_to_json_includes_key_limits_and_modes() {
    let mut cli = test_cli();
    cli.bash_profile = CliBashProfile::Strict;
    cli.os_sandbox_mode = CliOsSandboxMode::Auto;
    cli.max_file_write_bytes = 4096;
    cli.extension_runtime_hooks = true;
    cli.extension_runtime_root = PathBuf::from("/tmp/policy-overrides");

    let policy = build_tool_policy(&cli).expect("policy should build");
    let payload = tool_policy_to_json(&policy);
    assert_eq!(payload["schema_version"], 2);
    assert_eq!(payload["preset"], "balanced");
    assert_eq!(payload["bash_profile"], "strict");
    assert_eq!(payload["os_sandbox_mode"], "auto");
    assert_eq!(payload["max_file_write_bytes"], 4096);
    assert_eq!(payload["enforce_regular_files"], true);
    assert_eq!(payload["bash_dry_run"], false);
    assert_eq!(payload["tool_policy_trace"], false);
    assert_eq!(
        payload["extension_policy_override_root"],
        "/tmp/policy-overrides"
    );
}

#[test]
fn functional_build_tool_policy_hardened_preset_applies_hardened_defaults() {
    let mut cli = test_cli();
    cli.bash_timeout_ms = 120_000;
    cli.max_tool_output_bytes = 16_000;
    cli.max_file_read_bytes = 1_000_000;
    cli.max_file_write_bytes = 1_000_000;
    cli.max_command_length = 4_096;
    cli.allow_command_newlines = false;
    cli.bash_profile = CliBashProfile::Balanced;
    cli.os_sandbox_mode = CliOsSandboxMode::Off;
    cli.enforce_regular_files = true;
    cli.tool_policy_preset = CliToolPolicyPreset::Hardened;

    let policy = build_tool_policy(&cli).expect("policy should build");
    assert_eq!(policy.policy_preset, ToolPolicyPreset::Hardened);
    assert_eq!(policy.bash_profile, BashCommandProfile::Strict);
    assert_eq!(policy.max_command_length, 1_024);
    assert_eq!(policy.max_command_output_bytes, 4_000);
    assert_eq!(policy.os_sandbox_mode, OsSandboxMode::Force);
}

#[test]
fn regression_build_tool_policy_explicit_profile_overrides_preset_profile() {
    let mut cli = test_cli();
    cli.bash_timeout_ms = 120_000;
    cli.max_tool_output_bytes = 16_000;
    cli.max_file_read_bytes = 1_000_000;
    cli.max_file_write_bytes = 1_000_000;
    cli.max_command_length = 4_096;
    cli.allow_command_newlines = false;
    cli.os_sandbox_mode = CliOsSandboxMode::Off;
    cli.enforce_regular_files = true;
    cli.tool_policy_preset = CliToolPolicyPreset::Hardened;
    cli.bash_profile = CliBashProfile::Permissive;

    let policy = build_tool_policy(&cli).expect("policy should build");
    assert_eq!(policy.policy_preset, ToolPolicyPreset::Hardened);
    assert_eq!(policy.bash_profile, BashCommandProfile::Permissive);
    assert!(policy.allowed_commands.is_empty());
}

#[test]
fn functional_build_tool_policy_enables_trace_when_flag_set() {
    let mut cli = test_cli();
    cli.tool_policy_trace = true;
    let policy = build_tool_policy(&cli).expect("policy should build");
    assert!(policy.tool_policy_trace);
}

#[test]
fn functional_build_tool_policy_enables_extension_policy_override_with_runtime_hooks() {
    let mut cli = test_cli();
    cli.extension_runtime_hooks = true;
    cli.extension_runtime_root = PathBuf::from("/tmp/extensions-runtime");
    let policy = build_tool_policy(&cli).expect("policy should build");
    assert_eq!(
        policy.extension_policy_override_root.as_deref(),
        Some(Path::new("/tmp/extensions-runtime"))
    );
}

#[test]
fn functional_build_tool_policy_applies_strict_profile_and_custom_allowlist() {
    let mut cli = test_cli();
    cli.bash_profile = CliBashProfile::Strict;
    cli.allow_command = vec!["python".to_string(), "cargo-nextest*".to_string()];

    let policy = build_tool_policy(&cli).expect("policy should build");
    assert_eq!(policy.bash_profile, BashCommandProfile::Strict);
    assert!(policy.allowed_commands.contains(&"python".to_string()));
    assert!(policy
        .allowed_commands
        .contains(&"cargo-nextest*".to_string()));
    assert!(!policy.allowed_commands.contains(&"rm".to_string()));
}

#[test]
fn regression_build_tool_policy_permissive_profile_disables_allowlist() {
    let mut cli = test_cli();
    cli.bash_profile = CliBashProfile::Permissive;
    let policy = build_tool_policy(&cli).expect("policy should build");
    assert!(policy.allowed_commands.is_empty());
}

#[test]
fn regression_build_tool_policy_keeps_policy_override_disabled_without_runtime_hooks() {
    let mut cli = test_cli();
    cli.extension_runtime_root = PathBuf::from("/tmp/extensions-runtime");
    let policy = build_tool_policy(&cli).expect("policy should build");
    assert!(policy.extension_policy_override_root.is_none());
}

#[test]
fn functional_build_tool_policy_applies_sandbox_and_regular_file_settings() {
    let mut cli = test_cli();
    cli.os_sandbox_mode = CliOsSandboxMode::Auto;
    cli.os_sandbox_command = vec![
        "sandbox-run".to_string(),
        "--cwd".to_string(),
        "{cwd}".to_string(),
    ];
    cli.max_file_write_bytes = 4096;
    cli.enforce_regular_files = false;

    let policy = build_tool_policy(&cli).expect("policy should build");
    assert_eq!(policy.os_sandbox_mode, OsSandboxMode::Auto);
    assert_eq!(
        policy.os_sandbox_command,
        vec![
            "sandbox-run".to_string(),
            "--cwd".to_string(),
            "{cwd}".to_string()
        ]
    );
    assert_eq!(policy.max_file_write_bytes, 4096);
    assert!(!policy.enforce_regular_files);
}
