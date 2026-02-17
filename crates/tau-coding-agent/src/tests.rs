//! Crate-level regression/integration test harness for coding-agent runtime.
//!
//! These tests validate command dispatch, startup modes, and transport/routing
//! behaviors against deterministic fixtures and failure contracts.

use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    future::{pending, ready},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
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
use tau_cli::cli_args::{CliExecutionDomainFlags, CliGatewayDaemonFlags, CliRuntimeTailFlags};
use tau_cli::CliPromptSanitizerMode;
use tempfile::tempdir;
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::sleep;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use super::{
    apply_trust_root_mutations, branch_alias_path_for_session, build_auth_command_config,
    build_doctor_command_config, build_multi_channel_incident_timeline_report,
    build_multi_channel_route_inspect_report, build_profile_defaults, build_provider_client,
    build_tool_policy, command_file_error_mode_label, compose_startup_system_prompt,
    compute_session_entry_depths, compute_session_stats, current_unix_timestamp,
    decrypt_credential_store_secret, default_macro_config_path, default_profile_store_path,
    default_skills_lock_path, derive_skills_prune_candidates, encrypt_credential_store_secret,
    ensure_non_empty_text, escape_graph_label, evaluate_multi_channel_live_readiness,
    execute_auth_command, execute_branch_alias_command, execute_channel_store_admin_command,
    execute_command_file, execute_doctor_cli_command, execute_doctor_command,
    execute_doctor_command_with_options, execute_integration_auth_command, execute_macro_command,
    execute_package_activate_command, execute_package_activate_on_startup,
    execute_package_conflicts_command, execute_package_install_command,
    execute_package_list_command, execute_package_remove_command, execute_package_rollback_command,
    execute_package_show_command, execute_package_update_command, execute_package_validate_command,
    execute_profile_command, execute_rpc_capabilities_command, execute_rpc_dispatch_frame_command,
    execute_rpc_dispatch_ndjson_command, execute_rpc_serve_ndjson_command,
    execute_rpc_validate_frame_command, execute_session_bookmark_command,
    execute_session_diff_command, execute_session_graph_export_command,
    execute_session_search_command, execute_session_stats_command, execute_skills_list_command,
    execute_skills_lock_diff_command, execute_skills_lock_write_command,
    execute_skills_prune_command, execute_skills_search_command, execute_skills_show_command,
    execute_skills_sync_command, execute_skills_trust_add_command,
    execute_skills_trust_list_command, execute_skills_trust_revoke_command,
    execute_skills_trust_rotate_command, execute_skills_verify_command, execute_startup_preflight,
    format_id_list, format_remap_ids, handle_command, handle_command_with_session_import_mode,
    initialize_session, is_retryable_provider_error, load_branch_aliases, load_credential_store,
    load_macro_file, load_multi_agent_route_table, load_profile_store, load_session_bookmarks,
    load_trust_root_records, normalize_daemon_subcommand_args, parse_auth_command,
    parse_branch_alias_command, parse_command, parse_command_file, parse_doctor_command_args,
    parse_integration_auth_command, parse_macro_command, parse_numbered_plan_steps,
    parse_profile_command, parse_sandbox_command_tokens, parse_session_bookmark_command,
    parse_session_diff_args, parse_session_search_args, parse_session_stats_args,
    parse_skills_lock_diff_args, parse_skills_prune_args, parse_skills_search_args,
    parse_skills_trust_list_args, parse_skills_trust_mutation_args, parse_skills_verify_args,
    parse_trust_rotation_spec, parse_trusted_root_spec, percentile_duration_ms,
    provider_auth_capability, refresh_provider_access_token,
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
    run_doctor_checks, run_doctor_checks_with_lookup, run_plan_first_prompt,
    run_plan_first_prompt_with_policy_context,
    run_plan_first_prompt_with_policy_context_and_routing, run_prompt_with_cancellation,
    save_branch_aliases, save_credential_store, save_macro_file, save_profile_store,
    save_session_bookmarks, search_session_entries, session_bookmark_path_for_session,
    session_lineage_messages, session_message_preview, shared_lineage_prefix_depth,
    stream_text_chunks, summarize_audit_file, tool_audit_event_json, tool_policy_to_json,
    trust_record_status, unknown_command_message, validate_branch_alias_name,
    validate_custom_command_contract_runner_cli, validate_daemon_cli,
    validate_dashboard_contract_runner_cli, validate_deployment_contract_runner_cli,
    validate_deployment_wasm_inspect_cli, validate_deployment_wasm_package_cli,
    validate_event_webhook_ingest_cli, validate_events_runner_cli,
    validate_gateway_contract_runner_cli, validate_gateway_openresponses_server_cli,
    validate_gateway_remote_plan_cli, validate_gateway_remote_profile_inspect_cli,
    validate_gateway_service_cli, validate_github_issues_bridge_cli, validate_macro_command_entry,
    validate_macro_name, validate_memory_contract_runner_cli,
    validate_multi_agent_contract_runner_cli, validate_multi_channel_channel_lifecycle_cli,
    validate_multi_channel_contract_runner_cli, validate_multi_channel_incident_timeline_cli,
    validate_multi_channel_live_connectors_runner_cli, validate_multi_channel_live_ingest_cli,
    validate_multi_channel_live_runner_cli, validate_multi_channel_send_cli, validate_profile_name,
    validate_project_index_cli, validate_rpc_frame_file, validate_session_file,
    validate_skills_prune_file_name, validate_slack_bridge_cli, validate_voice_contract_runner_cli,
    validate_voice_live_runner_cli, AuthCommand, AuthCommandConfig, BranchAliasCommand,
    BranchAliasFile, Cli, CliBashProfile, CliCommandFileErrorMode,
    CliCredentialStoreEncryptionMode, CliDaemonProfile, CliDeploymentWasmBrowserDidMethod,
    CliDeploymentWasmRuntimeProfile, CliEventTemplateSchedule, CliGatewayOpenResponsesAuthMode,
    CliGatewayRemoteProfile, CliMultiChannelLiveConnectorMode, CliMultiChannelOutboundMode,
    CliMultiChannelTransport, CliOrchestratorMode, CliOsSandboxMode, CliProviderAuthMode,
    CliSessionImportMode, CliToolPolicyPreset, CliWebhookSignatureAlgorithm, ClientRoute,
    CommandAction, CommandExecutionContext, CommandFileEntry, CommandFileReport,
    CredentialStoreData, CredentialStoreEncryptionMode, DoctorCheckOptions, DoctorCheckResult,
    DoctorCommandArgs, DoctorCommandConfig, DoctorCommandOutputFormat,
    DoctorMultiChannelReadinessConfig, DoctorProviderKeyStatus, DoctorStatus,
    FallbackRoutingClient, IntegrationAuthCommand, IntegrationCredentialStoreRecord, MacroCommand,
    MacroFile, MultiAgentRouteTable, PlanFirstPromptPolicyRequest, PlanFirstPromptRequest,
    PlanFirstPromptRoutingRequest, ProfileCommand, ProfileDefaults, ProfileStoreFile,
    PromptRunStatus, PromptTelemetryLogger, ProviderAuthMethod, ProviderCredentialStoreRecord,
    RenderOptions, RuntimeExtensionHooksConfig, SessionBookmarkCommand, SessionBookmarkFile,
    SessionDiffEntry, SessionDiffReport, SessionGraphFormat, SessionRuntime, SessionSearchArgs,
    SessionStats, SessionStatsOutputFormat, SkillsPruneMode, SkillsSyncCommandConfig,
    SkillsVerifyEntry, SkillsVerifyReport, SkillsVerifyStatus, SkillsVerifySummary,
    SkillsVerifyTrustSummary, ToolAuditLogger, TrustedRootRecord, BRANCH_ALIAS_SCHEMA_VERSION,
    BRANCH_ALIAS_USAGE, MACRO_SCHEMA_VERSION, MACRO_USAGE, PROFILE_SCHEMA_VERSION, PROFILE_USAGE,
    SESSION_BOOKMARK_SCHEMA_VERSION, SESSION_BOOKMARK_USAGE, SESSION_SEARCH_DEFAULT_RESULTS,
    SESSION_SEARCH_PREVIEW_CHARS, SKILLS_PRUNE_USAGE, SKILLS_TRUST_ADD_USAGE,
    SKILLS_TRUST_LIST_USAGE, SKILLS_VERIFY_USAGE,
};
use crate::auth_commands::{
    auth_availability_counts, auth_mode_counts, auth_provider_counts, auth_revoked_counts,
    auth_source_kind, auth_source_kind_counts, auth_state_counts, auth_status_row_for_provider,
    format_auth_state_counts, AuthMatrixAvailabilityFilter, AuthMatrixModeSupportFilter,
    AuthRevokedFilter, AuthSourceKindFilter,
};
use crate::extension_manifest::discover_extension_runtime_registrations;
use crate::provider_api_key_candidates_with_inputs;
use crate::provider_auth_snapshot_for_status;
use crate::resolve_api_key;
use crate::tools::{register_extension_tools, BashCommandProfile, OsSandboxMode, ToolPolicyPreset};
use crate::{default_model_catalog_cache_path, ModelCatalog, MODELS_LIST_USAGE, MODEL_SHOW_USAGE};
use crate::{
    execute_extension_exec_command, execute_extension_list_command, execute_extension_show_command,
    execute_extension_validate_command,
};
use tau_session::{SessionImportMode, SessionStore};

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
        "schema_version": 6,
        "preset": "balanced",
        "allowed_roots": [],
        "allowed_commands": [],
        "bash_profile": "balanced",
        "os_sandbox_mode": "off",
        "os_sandbox_policy_mode": "best-effort",
        "bash_dry_run": false,
        "enforce_regular_files": true,
        "max_command_length": 4096,
        "max_command_output_bytes": 16000,
        "http_timeout_ms": 20000,
        "http_max_response_bytes": 256000,
        "http_max_redirects": 5,
        "http_allow_http": false,
        "http_allow_private_network": false,
    })
}

fn test_chat_request() -> ChatRequest {
    ChatRequest {
        model: "placeholder-model".to_string(),
        messages: vec![Message::user("hello")],
        tools: vec![],
        tool_choice: None,
        json_mode: false,
        max_tokens: None,
        temperature: None,
        prompt_cache: Default::default(),
    }
}

pub(crate) fn test_cli() -> Cli {
    Cli {
        shell_completion: None,
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
        provider_subscription_strict: false,
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
        agent_max_parallel_tool_calls: 4,
        agent_max_context_messages: Some(256),
        agent_request_max_retries: 2,
        agent_request_retry_initial_backoff_ms: 200,
        agent_request_retry_max_backoff_ms: 2_000,
        agent_cost_budget_usd: None,
        agent_cost_alert_threshold_percent: vec![80, 100],
        prompt_sanitizer_enabled: true,
        prompt_sanitizer_mode: CliPromptSanitizerMode::Warn,
        prompt_sanitizer_redaction_token: "[TAU-SAFETY-REDACTED]".to_string(),
        secret_leak_detector_enabled: true,
        secret_leak_detector_mode: CliPromptSanitizerMode::Warn,
        secret_leak_redaction_token: "[TAU-SECRET-REDACTED]".to_string(),
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
        orchestrator_route_table: None,
        prompt_file: None,
        prompt_template_file: None,
        prompt_template_var: vec![],
        command_file: None,
        command_file_error_mode: CliCommandFileErrorMode::FailFast,
        onboard: false,
        onboard_non_interactive: false,
        onboard_profile: "default".to_string(),
        onboard_release_channel: None,
        onboard_install_daemon: false,
        onboard_start_daemon: false,
        doctor_release_cache_file: PathBuf::from(".tau/release-lookup-cache.json"),
        doctor_release_cache_ttl_ms: 900_000,
        project_index_build: false,
        project_index_query: None,
        project_index_inspect: false,
        project_index_json: false,
        project_index_root: PathBuf::from("."),
        project_index_state_dir: PathBuf::from(".tau/index"),
        project_index_limit: 25,
        channel_store_root: PathBuf::from(".tau/channel-store"),
        channel_store_inspect: None,
        channel_store_repair: None,
        transport_health_inspect: None,
        transport_health_json: false,
        github_status_inspect: None,
        github_status_json: false,
        operator_control_summary: false,
        operator_control_summary_json: false,
        operator_control_summary_snapshot_out: None,
        operator_control_summary_compare: None,
        dashboard_status_inspect: false,
        dashboard_status_json: false,
        multi_channel_status_inspect: false,
        multi_channel_status_json: false,
        multi_channel_route_inspect_file: None,
        multi_channel_route_inspect_json: false,
        multi_channel_incident_timeline: false,
        multi_channel_incident_timeline_json: false,
        multi_channel_incident_start_unix_ms: None,
        multi_channel_incident_end_unix_ms: None,
        multi_channel_incident_event_limit: None,
        multi_channel_incident_replay_export: None,
        multi_agent_status_inspect: false,
        multi_agent_status_json: false,
        gateway_status_inspect: false,
        gateway_status_json: false,
        runtime_tail: CliRuntimeTailFlags {
            gateway_daemon: CliGatewayDaemonFlags {
                gateway_remote_profile_inspect: false,
                gateway_remote_profile_json: false,
                gateway_remote_plan: false,
                gateway_remote_plan_json: false,
                gateway_service_start: false,
                gateway_service_stop: false,
                gateway_service_stop_reason: None,
                gateway_service_status: false,
                gateway_service_status_json: false,
                daemon_install: false,
                daemon_uninstall: false,
                daemon_start: false,
                daemon_stop: false,
                daemon_stop_reason: None,
                daemon_status: false,
                daemon_status_json: false,
                daemon_profile: CliDaemonProfile::Auto,
                daemon_state_dir: PathBuf::from(".tau/daemon"),
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
                session: PathBuf::from(".tau/sessions/default.sqlite"),
                no_session: false,
                session_validate: false,
                session_import_mode: CliSessionImportMode::Merge,
                branch_from: None,
                session_lock_wait_ms: 5_000,
                session_lock_stale_ms: 30_000,
                runtime_heartbeat_enabled: true,
                runtime_heartbeat_interval_ms: 5_000,
                runtime_heartbeat_state_path: PathBuf::from(".tau/runtime-heartbeat/state.json"),
                runtime_self_repair_enabled: true,
                runtime_self_repair_timeout_ms: 300_000,
                runtime_self_repair_max_retries: 2,
                runtime_self_repair_tool_builds_dir: PathBuf::from(".tau/tool-builds"),
                runtime_self_repair_orphan_max_age_seconds: 3_600,
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
                os_sandbox_policy_mode: None,
                os_sandbox_docker_enabled: false,
                os_sandbox_docker_image: "debian:stable-slim".to_string(),
                os_sandbox_docker_network: tau_cli::CliOsSandboxDockerNetwork::None,
                os_sandbox_docker_memory_mb: 256,
                os_sandbox_docker_cpus: "1.0".to_string(),
                os_sandbox_docker_pids_limit: 256,
                os_sandbox_docker_read_only_rootfs: true,
                os_sandbox_docker_env: vec![],
                http_timeout_ms: 20_000,
                http_max_response_bytes: 256_000,
                http_max_redirects: 5,
                http_allow_http: false,
                http_allow_private_network: false,
                enforce_regular_files: true,
            },
            custom_command_contract_runner: false,
            custom_command_fixture: PathBuf::from(
                "crates/tau-coding-agent/testdata/custom-command-contract/mixed-outcomes.json",
            ),
            custom_command_state_dir: PathBuf::from(".tau/custom-command"),
            custom_command_queue_limit: 64,
            custom_command_processed_case_cap: 10_000,
            custom_command_retry_max_attempts: 4,
            custom_command_retry_base_delay_ms: 0,
            custom_command_policy_require_approval: true,
            custom_command_policy_allow_shell: false,
            custom_command_policy_sandbox_profile: "restricted".to_string(),
            custom_command_policy_allowed_env: vec![],
            custom_command_policy_denied_env: vec![],
            voice_contract_runner: false,
            voice_fixture: PathBuf::from(
                "crates/tau-coding-agent/testdata/voice-contract/mixed-outcomes.json",
            ),
            voice_state_dir: PathBuf::from(".tau/voice"),
            voice_queue_limit: 64,
            voice_processed_case_cap: 10_000,
            voice_retry_max_attempts: 4,
            voice_retry_base_delay_ms: 0,
            voice_live_runner: false,
            voice_live_input: PathBuf::from(
                "crates/tau-coding-agent/testdata/voice-live/single-turn.json",
            ),
            voice_live_wake_word: "tau".to_string(),
            voice_live_max_turns: 64,
            voice_live_tts_output: true,
            github_issues_bridge: false,
            github_repo: None,
            github_token: None,
            github_token_id: None,
            github_bot_login: None,
            github_api_base: "https://api.github.com".to_string(),
            github_state_dir: PathBuf::from(".tau/github-issues"),
            github_poll_interval_seconds: 30,
            github_poll_once: false,
            github_required_label: vec![],
            github_issue_number: vec![],
            github_artifact_retention_days: 30,
            github_include_issue_body: false,
            github_include_edited_comments: true,
            github_processed_event_cap: 10_000,
            github_retry_max_attempts: 4,
            github_retry_base_delay_ms: 500,
        },
        deployment_status_inspect: false,
        deployment_status_json: false,
        custom_command_status_inspect: false,
        custom_command_status_json: false,
        voice_status_inspect: false,
        voice_status_json: false,
        extension_exec_manifest: None,
        extension_exec_hook: None,
        extension_exec_payload_file: None,
        extension_validate: None,
        extension_list: false,
        extension_list_root: PathBuf::from(".tau/extensions"),
        extension_show: None,
        extension_runtime_hooks: false,
        extension_runtime_root: PathBuf::from(".tau/extensions"),
        tool_builder_enabled: false,
        tool_builder_output_root: PathBuf::from(".tau/generated-tools"),
        tool_builder_extension_root: PathBuf::from(".tau/extensions/generated"),
        tool_builder_max_attempts: 3,
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
        qa_loop: false,
        qa_loop_config: None,
        qa_loop_json: false,
        qa_loop_stage_timeout_ms: None,
        qa_loop_retry_failures: None,
        qa_loop_max_output_bytes: None,
        qa_loop_changed_file_limit: None,
        prompt_optimization_config: None,
        prompt_optimization_store_sqlite: PathBuf::from(".tau/training/store.sqlite"),
        prompt_optimization_json: false,
        prompt_optimization_proxy_server: false,
        prompt_optimization_proxy_bind: "127.0.0.1:8788".to_string(),
        prompt_optimization_proxy_upstream_url: None,
        prompt_optimization_proxy_state_dir: PathBuf::from(".tau"),
        prompt_optimization_proxy_timeout_ms: 30_000,
        prompt_optimization_control_status: false,
        prompt_optimization_control_pause: false,
        prompt_optimization_control_resume: false,
        prompt_optimization_control_cancel: false,
        prompt_optimization_control_rollback: None,
        prompt_optimization_control_state_dir: PathBuf::from(".tau/training"),
        prompt_optimization_control_json: false,
        prompt_optimization_control_principal: None,
        prompt_optimization_control_rbac_policy: PathBuf::from(".tau/security/rbac.json"),
        mcp_server: false,
        mcp_client: false,
        mcp_client_inspect: false,
        mcp_client_inspect_json: false,
        mcp_external_server_config: None,
        mcp_context_provider: vec![],
        rpc_capabilities: false,
        rpc_validate_frame_file: None,
        rpc_dispatch_frame_file: None,
        rpc_dispatch_ndjson_file: None,
        rpc_serve_ndjson: false,
        execution_domain: CliExecutionDomainFlags {
            events_inspect: false,
            events_inspect_json: false,
            events_validate: false,
            events_validate_json: false,
            events_simulate: false,
            events_simulate_json: false,
            events_dry_run: false,
            events_dry_run_json: false,
            events_dry_run_strict: false,
            events_dry_run_max_error_rows: None,
            events_dry_run_max_execute_rows: None,
            events_simulate_horizon_seconds: 3_600,
            events_template_write: None,
            events_template_schedule: CliEventTemplateSchedule::Immediate,
            events_template_overwrite: false,
            events_template_id: None,
            events_template_channel: None,
            events_template_prompt: None,
            events_template_at_unix_ms: None,
            events_template_cron: None,
            events_template_timezone: "UTC".to_string(),
        },
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
        multi_channel_contract_runner: false,
        multi_channel_live_runner: false,
        multi_channel_live_connectors_runner: false,
        multi_channel_live_connectors_status: false,
        multi_channel_live_connectors_status_json: false,
        multi_channel_live_connectors_state_path: PathBuf::from(
            ".tau/multi-channel/live-connectors-state.json",
        ),
        multi_channel_live_connectors_poll_once: false,
        multi_channel_live_webhook_bind: "127.0.0.1:8788".to_string(),
        multi_channel_telegram_ingress_mode: CliMultiChannelLiveConnectorMode::Disabled,
        multi_channel_discord_ingress_mode: CliMultiChannelLiveConnectorMode::Disabled,
        multi_channel_whatsapp_ingress_mode: CliMultiChannelLiveConnectorMode::Disabled,
        multi_channel_discord_ingress_channel_ids: vec![],
        multi_channel_telegram_webhook_secret: None,
        multi_channel_whatsapp_webhook_verify_token: None,
        multi_channel_whatsapp_webhook_app_secret: None,
        multi_channel_live_ingest_file: None,
        multi_channel_live_ingest_transport: None,
        multi_channel_live_ingest_provider: "native-ingress".to_string(),
        multi_channel_live_ingest_dir: PathBuf::from(".tau/multi-channel/live-ingress"),
        multi_channel_live_readiness_preflight: false,
        multi_channel_live_readiness_json: false,
        multi_channel_channel_status: None,
        multi_channel_channel_status_json: false,
        multi_channel_channel_login: None,
        multi_channel_channel_login_json: false,
        multi_channel_channel_logout: None,
        multi_channel_channel_logout_json: false,
        multi_channel_channel_probe: None,
        multi_channel_channel_probe_json: false,
        multi_channel_channel_probe_online: false,
        multi_channel_send: None,
        multi_channel_send_target: None,
        multi_channel_send_text: None,
        multi_channel_send_text_file: None,
        multi_channel_send_json: false,
        multi_channel_fixture: PathBuf::from(
            "crates/tau-multi-channel/testdata/multi-channel-contract/baseline-three-channel.json",
        ),
        multi_channel_live_ingress_dir: PathBuf::from(".tau/multi-channel/live-ingress"),
        multi_channel_state_dir: PathBuf::from(".tau/multi-channel"),
        multi_channel_queue_limit: 64,
        multi_channel_processed_event_cap: 10_000,
        multi_channel_retry_max_attempts: 4,
        multi_channel_retry_base_delay_ms: 0,
        multi_channel_retry_jitter_ms: 0,
        multi_channel_telemetry_typing_presence: true,
        multi_channel_telemetry_usage_summary: true,
        multi_channel_telemetry_include_identifiers: false,
        multi_channel_telemetry_min_response_chars: 120,
        multi_channel_media_understanding: true,
        multi_channel_media_max_attachments: 4,
        multi_channel_media_max_summary_chars: 280,
        multi_channel_outbound_mode: CliMultiChannelOutboundMode::ChannelStore,
        multi_channel_outbound_max_chars: 1200,
        multi_channel_outbound_http_timeout_ms: 5000,
        multi_channel_outbound_ssrf_protection: true,
        multi_channel_outbound_ssrf_allow_http: false,
        multi_channel_outbound_ssrf_allow_private_network: false,
        multi_channel_outbound_max_redirects: 5,
        multi_channel_telegram_api_base: "https://api.telegram.org".to_string(),
        multi_channel_discord_api_base: "https://discord.com/api/v10".to_string(),
        multi_channel_whatsapp_api_base: "https://graph.facebook.com/v20.0".to_string(),
        multi_channel_telegram_bot_token: None,
        multi_channel_discord_bot_token: None,
        multi_channel_whatsapp_access_token: None,
        multi_channel_whatsapp_phone_number_id: None,
        multi_agent_contract_runner: false,
        multi_agent_fixture: PathBuf::from(
            "crates/tau-coding-agent/testdata/multi-agent-contract/mixed-outcomes.json",
        ),
        multi_agent_state_dir: PathBuf::from(".tau/multi-agent"),
        multi_agent_queue_limit: 64,
        multi_agent_processed_case_cap: 10_000,
        multi_agent_retry_max_attempts: 4,
        multi_agent_retry_base_delay_ms: 0,
        browser_automation_contract_runner: false,
        browser_automation_live_runner: false,
        browser_automation_live_fixture: PathBuf::from(
            "crates/tau-coding-agent/testdata/browser-automation-live/live-sequence.json",
        ),
        browser_automation_fixture: PathBuf::from(
            "crates/tau-coding-agent/testdata/browser-automation-contract/mixed-outcomes.json",
        ),
        browser_automation_state_dir: PathBuf::from(".tau/browser-automation"),
        browser_automation_queue_limit: 64,
        browser_automation_processed_case_cap: 10_000,
        browser_automation_retry_max_attempts: 4,
        browser_automation_retry_base_delay_ms: 0,
        browser_automation_action_timeout_ms: 5_000,
        browser_automation_max_actions_per_case: 8,
        browser_automation_allow_unsafe_actions: false,
        browser_automation_playwright_cli: "playwright-cli".to_string(),
        browser_automation_preflight: false,
        browser_automation_preflight_json: false,
        memory_contract_runner: false,
        memory_fixture: PathBuf::from(
            "crates/tau-memory/testdata/memory-contract/mixed-outcomes.json",
        ),
        memory_state_dir: PathBuf::from(".tau/memory"),
        jobs_enabled: true,
        jobs_state_dir: PathBuf::from(".tau/jobs"),
        jobs_list_default_limit: 20,
        jobs_list_max_limit: 200,
        jobs_default_timeout_ms: 30_000,
        jobs_max_timeout_ms: 900_000,
        memory_queue_limit: 64,
        memory_processed_case_cap: 10_000,
        memory_retry_max_attempts: 4,
        memory_retry_base_delay_ms: 0,
        dashboard_contract_runner: false,
        dashboard_fixture: PathBuf::from(
            "crates/tau-coding-agent/testdata/dashboard-contract/mixed-outcomes.json",
        ),
        dashboard_state_dir: PathBuf::from(".tau/dashboard"),
        dashboard_queue_limit: 64,
        dashboard_processed_case_cap: 10_000,
        dashboard_retry_max_attempts: 4,
        dashboard_retry_base_delay_ms: 0,
        gateway_openresponses_server: false,
        gateway_openresponses_bind: "127.0.0.1:8787".to_string(),
        gateway_remote_profile: CliGatewayRemoteProfile::LocalOnly,
        gateway_openresponses_auth_mode: CliGatewayOpenResponsesAuthMode::Token,
        gateway_openresponses_auth_token: None,
        gateway_openresponses_auth_password: None,
        gateway_openresponses_session_ttl_seconds: 3_600,
        gateway_openresponses_rate_limit_window_seconds: 60,
        gateway_openresponses_rate_limit_max_requests: 120,
        gateway_openresponses_max_input_chars: 32_000,
        gateway_contract_runner: false,
        gateway_fixture: PathBuf::from(
            "crates/tau-gateway/testdata/gateway-contract/mixed-outcomes.json",
        ),
        gateway_state_dir: PathBuf::from(".tau/gateway"),
        gateway_guardrail_failure_streak_threshold: 2,
        gateway_guardrail_retryable_failures_threshold: 2,
        deployment_contract_runner: false,
        deployment_fixture: PathBuf::from(
            "crates/tau-coding-agent/testdata/deployment-contract/mixed-outcomes.json",
        ),
        deployment_state_dir: PathBuf::from(".tau/deployment"),
        deployment_wasm_package_module: None,
        deployment_wasm_package_blueprint_id: "edge-wasm".to_string(),
        deployment_wasm_package_runtime_profile: CliDeploymentWasmRuntimeProfile::WasmWasi,
        deployment_wasm_package_output_dir: PathBuf::from(".tau/deployment/wasm-artifacts"),
        deployment_wasm_package_json: false,
        deployment_wasm_inspect_manifest: None,
        deployment_wasm_inspect_json: false,
        deployment_wasm_browser_did_init: false,
        deployment_wasm_browser_did_method: CliDeploymentWasmBrowserDidMethod::Key,
        deployment_wasm_browser_did_network: "tau-devnet".to_string(),
        deployment_wasm_browser_did_subject: "browser-agent".to_string(),
        deployment_wasm_browser_did_entropy: "tau-browser-seed".to_string(),
        deployment_wasm_browser_did_output: PathBuf::from(".tau/deployment/browser-did.json"),
        deployment_wasm_browser_did_json: false,
        deployment_queue_limit: 64,
        deployment_processed_case_cap: 10_000,
        deployment_retry_max_attempts: 4,
        deployment_retry_base_delay_ms: 0,
    }
}

fn parse_cli_with_stack<I, T>(args: I) -> Cli
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString>,
{
    let (owned_args, _) = normalize_startup_cli_args(
        args.into_iter()
            .map(Into::into)
            .map(|value: std::ffi::OsString| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
    );
    thread::Builder::new()
        .name("tau-cli-parse".to_string())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || Cli::parse_from(owned_args))
        .expect("spawn cli parse thread")
        .join()
        .expect("join cli parse thread")
}

fn try_parse_cli_with_stack<I, T>(args: I) -> Result<Cli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString>,
{
    let (owned_args, _) = normalize_startup_cli_args(
        args.into_iter()
            .map(Into::into)
            .map(|value: std::ffi::OsString| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
    );
    thread::Builder::new()
        .name("tau-cli-try-parse".to_string())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || Cli::try_parse_from(owned_args))
        .expect("spawn cli try-parse thread")
        .join()
        .expect("join cli try-parse thread")
}

fn normalize_startup_cli_args(args: Vec<String>) -> (Vec<String>, Vec<String>) {
    crate::normalize_startup_cli_args(args)
}

fn set_workspace_tau_paths(cli: &mut Cli, workspace: &Path) {
    let tau_root = workspace.join(".tau");
    cli.session = tau_root.join("sessions/default.sqlite");
    cli.credential_store = tau_root.join("credentials.json");
    cli.skills_dir = tau_root.join("skills");
    cli.model_catalog_cache = tau_root.join("models/catalog.json");
    cli.channel_store_root = tau_root.join("channel-store");
    cli.events_dir = tau_root.join("events");
    cli.events_state_path = tau_root.join("events/state.json");
    cli.runtime_heartbeat_state_path = tau_root.join("runtime-heartbeat/state.json");
    cli.multi_channel_state_dir = tau_root.join("multi-channel");
    cli.multi_agent_state_dir = tau_root.join("multi-agent");
    cli.browser_automation_state_dir = tau_root.join("browser-automation");
    cli.memory_state_dir = tau_root.join("memory");
    cli.dashboard_state_dir = tau_root.join("dashboard");
    cli.gateway_state_dir = tau_root.join("gateway");
    cli.daemon_state_dir = tau_root.join("daemon");
    cli.deployment_state_dir = tau_root.join("deployment");
    cli.deployment_wasm_package_output_dir = tau_root.join("deployment/wasm-artifacts");
    cli.custom_command_state_dir = tau_root.join("custom-command");
    cli.voice_state_dir = tau_root.join("voice");
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
            release_channel_path: PathBuf::from(".tau/release-channel.json"),
            release_lookup_cache_path: PathBuf::from(".tau/release-lookup-cache.json"),
            release_lookup_cache_ttl_ms: 900_000,
            browser_automation_playwright_cli: "playwright-cli".to_string(),
            session_enabled: true,
            session_path: PathBuf::from(".tau/sessions/default.sqlite"),
            skills_dir: skills_dir.to_path_buf(),
            skills_lock_path: lock_path.to_path_buf(),
            trust_root_path: trust_root_path.map(Path::to_path_buf),
            multi_channel_live_readiness: DoctorMultiChannelReadinessConfig::default(),
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
        Provider::OpenAi | Provider::OpenRouter => config.openai_auth_mode = mode,
        Provider::Anthropic => config.anthropic_auth_mode = mode,
        Provider::Google => config.google_auth_mode = mode,
    }
}

fn set_provider_api_key(config: &mut AuthCommandConfig, provider: Provider, value: &str) {
    match provider {
        Provider::OpenAi | Provider::OpenRouter => config.openai_api_key = Some(value.to_string()),
        Provider::Anthropic => config.anthropic_api_key = Some(value.to_string()),
        Provider::Google => config.google_api_key = Some(value.to_string()),
    }
}

#[path = "tests/auth_provider/mod.rs"]
mod auth_provider;
#[path = "tests/cli_validation.rs"]
mod cli_validation;
#[path = "tests/extensions_rpc.rs"]
mod extensions_rpc;
#[path = "tests/misc.rs"]
mod misc;
#[path = "tests/runtime_agent.rs"]
mod runtime_agent;
