mod approvals;
mod atomic_io;
mod auth_commands;
mod auth_types;
mod bootstrap_helpers;
mod canvas;
mod channel_store;
mod channel_store_admin;
mod claude_cli_client;
mod cli_args;
mod cli_executable;
mod cli_types;
mod codex_cli_client;
mod commands;
mod credentials;
mod custom_command_contract;
mod custom_command_runtime;
mod dashboard_contract;
mod dashboard_runtime;
mod deployment_contract;
mod deployment_runtime;
mod diagnostics_commands;
mod events;
mod extension_manifest;
mod gateway_contract;
mod gateway_runtime;
mod gemini_cli_client;
mod github_issues;
mod github_issues_helpers;
mod github_transport_helpers;
mod macro_profile_commands;
mod mcp_server;
mod memory_contract;
mod memory_runtime;
mod model_catalog;
mod multi_agent_contract;
mod multi_agent_router;
mod multi_agent_runtime;
mod multi_channel_contract;
mod multi_channel_live_ingress;
mod multi_channel_runtime;
mod observability_loggers;
mod onboarding;
mod orchestrator;
mod package_manifest;
mod pairing;
mod provider_auth;
mod provider_client;
mod provider_credentials;
mod provider_fallback;
mod qa_loop_commands;
mod rbac;
mod release_channel_commands;
mod rpc_capabilities;
mod rpc_protocol;
mod runtime_cli_validation;
mod runtime_loop;
mod runtime_output;
mod runtime_types;
mod session;
mod session_commands;
mod session_graph_commands;
mod session_navigation_commands;
mod session_runtime_helpers;
mod skills;
mod skills_commands;
mod slack;
mod slack_helpers;
mod startup_config;
mod startup_dispatch;
mod startup_local_runtime;
mod startup_model_catalog;
mod startup_model_resolution;
mod startup_policy;
mod startup_preflight;
mod startup_prompt_composition;
mod startup_resolution;
mod startup_skills_bootstrap;
mod startup_transport_modes;
mod time_utils;
mod tool_policy_config;
mod tools;
#[cfg(test)]
mod transport_conformance;
mod transport_health;
mod trust_roots;
mod voice_contract;
mod voice_runtime;

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use clap::Parser;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tau_agent_core::{Agent, AgentConfig, AgentEvent};
use tau_ai::{
    AnthropicClient, AnthropicConfig, ChatRequest, ChatResponse, GoogleClient, GoogleConfig,
    LlmClient, Message, MessageRole, ModelRef, OpenAiAuthScheme, OpenAiClient, OpenAiConfig,
    Provider, StreamDeltaHandler, TauAiError,
};

pub(crate) use crate::approvals::{
    evaluate_approval_gate, execute_approvals_command, ApprovalAction, ApprovalGateResult,
    APPROVALS_USAGE,
};
pub(crate) use crate::atomic_io::write_text_atomic;
pub(crate) use crate::auth_commands::execute_auth_command;
#[cfg(test)]
pub(crate) use crate::auth_commands::{parse_auth_command, AuthCommand};
pub(crate) use crate::auth_types::{CredentialStoreEncryptionMode, ProviderAuthMethod};
pub(crate) use crate::bootstrap_helpers::{command_file_error_mode_label, init_tracing};
pub(crate) use crate::canvas::{
    execute_canvas_command, CanvasCommandConfig, CanvasEventOrigin, CanvasSessionLinkContext,
    CANVAS_USAGE,
};
use crate::channel_store::ChannelStore;
pub(crate) use crate::channel_store_admin::execute_channel_store_admin_command;
pub(crate) use crate::cli_args::Cli;
pub(crate) use crate::cli_types::{
    CliBashProfile, CliCommandFileErrorMode, CliCredentialStoreEncryptionMode,
    CliEventTemplateSchedule, CliMultiChannelTransport, CliOrchestratorMode, CliOsSandboxMode,
    CliProviderAuthMode, CliSessionImportMode, CliToolPolicyPreset, CliWebhookSignatureAlgorithm,
};
#[cfg(test)]
pub(crate) use crate::commands::handle_command;
pub(crate) use crate::commands::{
    canonical_command_name, execute_command_file, handle_command_with_session_import_mode,
    parse_command, CommandAction, COMMAND_NAMES,
};
#[cfg(test)]
pub(crate) use crate::commands::{
    parse_command_file, render_command_help, render_help_overview, unknown_command_message,
    CommandFileEntry, CommandFileReport,
};
#[cfg(test)]
pub(crate) use crate::credentials::{
    decrypt_credential_store_secret, encrypt_credential_store_secret,
    parse_integration_auth_command, IntegrationAuthCommand, IntegrationCredentialStoreRecord,
};
pub(crate) use crate::credentials::{
    execute_integration_auth_command, load_credential_store, reauth_required_error,
    refresh_provider_access_token, resolve_credential_store_encryption_mode,
    resolve_non_empty_cli_value, resolve_secret_from_cli_or_store_id, save_credential_store,
    CredentialStoreData, ProviderCredentialStoreRecord,
};
pub(crate) use crate::diagnostics_commands::{
    build_doctor_command_config, execute_audit_summary_command, execute_doctor_cli_command,
    execute_multi_channel_live_readiness_preflight_command, execute_policy_command,
};
#[cfg(test)]
pub(crate) use crate::diagnostics_commands::{
    evaluate_multi_channel_live_readiness, parse_doctor_command_args, percentile_duration_ms,
    render_audit_summary, render_doctor_report, render_doctor_report_json, run_doctor_checks,
    run_doctor_checks_with_lookup, summarize_audit_file, DoctorCheckOptions, DoctorCheckResult,
    DoctorCommandArgs, DoctorCommandOutputFormat, DoctorStatus,
};
#[cfg(test)]
pub(crate) use crate::diagnostics_commands::{
    execute_doctor_command, execute_doctor_command_with_options,
};
use crate::events::{
    execute_events_dry_run_command, execute_events_inspect_command,
    execute_events_simulate_command, execute_events_template_write_command,
    execute_events_validate_command, ingest_webhook_immediate_event, run_event_scheduler,
    EventSchedulerConfig, EventWebhookIngestConfig,
};
pub(crate) use crate::extension_manifest::{
    apply_extension_message_transforms, dispatch_extension_runtime_hook,
    execute_extension_exec_command, execute_extension_list_command, execute_extension_show_command,
    execute_extension_validate_command,
};
pub(crate) use crate::macro_profile_commands::{
    default_macro_config_path, default_profile_store_path, execute_macro_command,
    execute_profile_command,
};
#[cfg(test)]
pub(crate) use crate::macro_profile_commands::{
    load_macro_file, load_profile_store, parse_macro_command, parse_profile_command,
    render_macro_list, render_macro_show, render_profile_diffs, render_profile_list,
    render_profile_show, save_macro_file, save_profile_store, validate_macro_command_entry,
    validate_macro_name, validate_profile_name, MacroCommand, MacroFile, ProfileCommand,
    ProfileStoreFile, MACRO_SCHEMA_VERSION, MACRO_USAGE, PROFILE_SCHEMA_VERSION, PROFILE_USAGE,
};
pub(crate) use crate::mcp_server::execute_mcp_server_command;
#[cfg(test)]
pub(crate) use crate::model_catalog::default_model_catalog_cache_path;
pub(crate) use crate::model_catalog::{
    ensure_model_supports_tools, load_model_catalog_with_cache, parse_models_list_args,
    render_model_show, render_models_list, ModelCatalog, ModelCatalogLoadOptions,
    MODELS_LIST_USAGE, MODEL_SHOW_USAGE,
};
pub(crate) use crate::multi_agent_router::{
    build_multi_agent_role_prompt, load_multi_agent_route_table, resolve_multi_agent_role_profile,
    select_multi_agent_route, MultiAgentRoutePhase, MultiAgentRouteTable,
};
#[cfg(test)]
pub(crate) use crate::observability_loggers::tool_audit_event_json;
pub(crate) use crate::observability_loggers::{PromptTelemetryLogger, ToolAuditLogger};
pub(crate) use crate::onboarding::execute_onboarding_command;
#[cfg(test)]
pub(crate) use crate::orchestrator::parse_numbered_plan_steps;
pub(crate) use crate::orchestrator::run_plan_first_prompt;
pub(crate) use crate::orchestrator::run_plan_first_prompt_with_policy_context;
pub(crate) use crate::orchestrator::run_plan_first_prompt_with_policy_context_and_routing;
pub(crate) use crate::package_manifest::{
    execute_package_activate_command, execute_package_activate_on_startup,
    execute_package_conflicts_command, execute_package_install_command,
    execute_package_list_command, execute_package_remove_command, execute_package_rollback_command,
    execute_package_show_command, execute_package_update_command, execute_package_validate_command,
};
pub(crate) use crate::pairing::{
    evaluate_pairing_access, execute_pair_command, execute_unpair_command,
    pairing_policy_for_state_dir, PairingDecision,
};
pub(crate) use crate::provider_auth::{
    configured_provider_auth_method, configured_provider_auth_method_from_config,
    missing_provider_api_key_message, provider_api_key_candidates,
    provider_api_key_candidates_with_inputs, provider_auth_capability, provider_auth_mode_flag,
    resolve_api_key,
};
pub(crate) use crate::provider_client::build_provider_client;
#[cfg(test)]
pub(crate) use crate::provider_credentials::resolve_store_backed_provider_credential;
pub(crate) use crate::provider_credentials::{
    resolve_non_empty_secret_with_source, CliProviderCredentialResolver, ProviderCredentialResolver,
};
pub(crate) use crate::provider_fallback::{build_client_with_fallbacks, resolve_fallback_models};
#[cfg(test)]
pub(crate) use crate::provider_fallback::{
    is_retryable_provider_error, ClientRoute, FallbackRoutingClient,
};
pub(crate) use crate::qa_loop_commands::{
    execute_qa_loop_cli_command, execute_qa_loop_preflight_command, QA_LOOP_USAGE,
};
pub(crate) use crate::rbac::{
    authorize_action_for_principal_with_policy_path, authorize_command_for_principal,
    authorize_tool_for_principal, authorize_tool_for_principal_with_policy_path,
    execute_rbac_command, github_principal, rbac_policy_path_for_state_dir,
    resolve_local_principal, slack_principal, RbacDecision, RBAC_USAGE,
};
pub(crate) use crate::release_channel_commands::{
    default_release_channel_path, execute_release_channel_command, load_release_channel_store,
    RELEASE_CHANNEL_USAGE,
};
pub(crate) use crate::rpc_capabilities::execute_rpc_capabilities_command;
#[cfg(test)]
pub(crate) use crate::rpc_capabilities::rpc_capabilities_payload;
#[cfg(test)]
pub(crate) use crate::rpc_protocol::validate_rpc_frame_file;
pub(crate) use crate::rpc_protocol::{
    execute_rpc_dispatch_frame_command, execute_rpc_dispatch_ndjson_command,
    execute_rpc_serve_ndjson_command, execute_rpc_validate_frame_command,
};
pub(crate) use crate::runtime_cli_validation::{
    validate_custom_command_contract_runner_cli, validate_dashboard_contract_runner_cli,
    validate_deployment_contract_runner_cli, validate_event_webhook_ingest_cli,
    validate_events_runner_cli, validate_gateway_contract_runner_cli, validate_gateway_service_cli,
    validate_github_issues_bridge_cli, validate_memory_contract_runner_cli,
    validate_multi_agent_contract_runner_cli, validate_multi_channel_contract_runner_cli,
    validate_multi_channel_live_ingest_cli, validate_multi_channel_live_runner_cli,
    validate_slack_bridge_cli, validate_voice_contract_runner_cli,
};
pub(crate) use crate::runtime_loop::{
    resolve_prompt_input, run_interactive, run_plan_first_prompt_with_runtime_hooks, run_prompt,
    run_prompt_with_cancellation, InteractiveRuntimeConfig, PromptRunStatus,
    RuntimeExtensionHooksConfig,
};
#[cfg(test)]
pub(crate) use crate::runtime_output::stream_text_chunks;
pub(crate) use crate::runtime_output::{
    event_to_json, persist_messages, print_assistant_messages, summarize_message,
};
pub(crate) use crate::runtime_types::{
    AuthCommandConfig, CommandExecutionContext, DoctorCommandConfig,
    DoctorMultiChannelReadinessConfig, DoctorProviderKeyStatus, ProfileAuthDefaults,
    ProfileDefaults, ProfileMcpDefaults, ProfilePolicyDefaults, ProfileSessionDefaults,
    RenderOptions, SessionRuntime, SkillsSyncCommandConfig,
};
use crate::session::{SessionImportMode, SessionStore};
#[cfg(test)]
pub(crate) use crate::session_commands::{
    compute_session_entry_depths, compute_session_stats, execute_session_diff_command,
    execute_session_search_command, execute_session_stats_command, parse_session_diff_args,
    parse_session_search_args, parse_session_stats_args, render_session_diff, render_session_stats,
    render_session_stats_json, search_session_entries, shared_lineage_prefix_depth,
    SessionDiffEntry, SessionDiffReport, SessionSearchArgs, SessionStats, SessionStatsOutputFormat,
    SESSION_SEARCH_DEFAULT_RESULTS, SESSION_SEARCH_PREVIEW_CHARS,
};
pub(crate) use crate::session_commands::{session_message_preview, session_message_role};
pub(crate) use crate::session_graph_commands::execute_session_graph_export_command;
#[cfg(test)]
pub(crate) use crate::session_graph_commands::{
    escape_graph_label, render_session_graph_dot, render_session_graph_mermaid,
    resolve_session_graph_format, SessionGraphFormat,
};
#[cfg(test)]
pub(crate) use crate::session_navigation_commands::{
    branch_alias_path_for_session, load_branch_aliases, load_session_bookmarks,
    parse_branch_alias_command, parse_session_bookmark_command, save_branch_aliases,
    save_session_bookmarks, session_bookmark_path_for_session, validate_branch_alias_name,
    BranchAliasCommand, BranchAliasFile, SessionBookmarkCommand, SessionBookmarkFile,
    BRANCH_ALIAS_SCHEMA_VERSION, BRANCH_ALIAS_USAGE, SESSION_BOOKMARK_SCHEMA_VERSION,
    SESSION_BOOKMARK_USAGE,
};
pub(crate) use crate::session_navigation_commands::{
    execute_branch_alias_command, execute_session_bookmark_command,
};
pub(crate) use crate::session_runtime_helpers::{
    format_id_list, format_remap_ids, initialize_session, reload_agent_from_active_head,
    validate_session_file,
};
use crate::skills::{
    augment_system_prompt, build_local_skill_lock_hints, build_registry_skill_lock_hints,
    build_remote_skill_lock_hints, default_skills_cache_dir, default_skills_lock_path,
    fetch_registry_manifest_with_cache, install_remote_skills_with_cache, install_skills,
    load_catalog, load_skills_lockfile, resolve_registry_skill_sources,
    resolve_remote_skill_sources, resolve_selected_skills, sync_skills_with_lockfile,
    write_skills_lockfile, SkillsDownloadOptions, TrustedKey,
};
#[cfg(test)]
pub(crate) use crate::skills_commands::{
    derive_skills_prune_candidates, parse_skills_lock_diff_args, parse_skills_prune_args,
    parse_skills_search_args, parse_skills_trust_list_args, parse_skills_trust_mutation_args,
    parse_skills_verify_args, render_skills_list, render_skills_lock_diff_drift,
    render_skills_lock_diff_in_sync, render_skills_search, render_skills_show,
    render_skills_trust_list, render_skills_verify_report, resolve_prunable_skill_file_name,
    resolve_skills_lock_path, trust_record_status, validate_skills_prune_file_name,
    SkillsPruneMode, SkillsVerifyEntry, SkillsVerifyReport, SkillsVerifyStatus,
    SkillsVerifySummary, SkillsVerifyTrustSummary, SKILLS_PRUNE_USAGE, SKILLS_TRUST_ADD_USAGE,
    SKILLS_TRUST_LIST_USAGE, SKILLS_VERIFY_USAGE,
};
pub(crate) use crate::skills_commands::{
    execute_skills_list_command, execute_skills_lock_diff_command,
    execute_skills_lock_write_command, execute_skills_prune_command, execute_skills_search_command,
    execute_skills_show_command, execute_skills_sync_command, execute_skills_trust_add_command,
    execute_skills_trust_list_command, execute_skills_trust_revoke_command,
    execute_skills_trust_rotate_command, execute_skills_verify_command,
    render_skills_lock_write_success, render_skills_sync_drift_details, render_skills_sync_in_sync,
};
pub(crate) use crate::startup_config::{
    build_auth_command_config, build_profile_defaults, default_provider_auth_method,
};
use crate::startup_dispatch::run_cli;
#[cfg(test)]
pub(crate) use crate::startup_local_runtime::register_runtime_extension_tool_hook_subscriber;
pub(crate) use crate::startup_local_runtime::{run_local_runtime, LocalRuntimeConfig};
pub(crate) use crate::startup_model_catalog::{
    resolve_startup_model_catalog, validate_startup_model_catalog,
};
pub(crate) use crate::startup_model_resolution::{resolve_startup_models, StartupModelResolution};
pub(crate) use crate::startup_policy::{resolve_startup_policy, StartupPolicyBundle};
pub(crate) use crate::startup_preflight::execute_startup_preflight;
pub(crate) use crate::startup_prompt_composition::compose_startup_system_prompt;
pub(crate) use crate::startup_resolution::{
    ensure_non_empty_text, resolve_skill_trust_roots, resolve_system_prompt,
};
pub(crate) use crate::startup_skills_bootstrap::run_startup_skills_bootstrap;
pub(crate) use crate::startup_transport_modes::run_transport_mode_if_requested;
pub(crate) use crate::time_utils::{
    current_unix_timestamp, current_unix_timestamp_ms, is_expired_unix,
};
#[cfg(test)]
pub(crate) use crate::tool_policy_config::parse_sandbox_command_tokens;
pub(crate) use crate::tool_policy_config::{build_tool_policy, tool_policy_to_json};
use crate::tools::{tool_policy_preset_name, ToolPolicy};
pub(crate) use crate::transport_health::TransportHealthSnapshot;
pub(crate) use crate::trust_roots::{
    apply_trust_root_mutation_specs, apply_trust_root_mutations, load_trust_root_records,
    parse_trust_rotation_spec, parse_trusted_root_spec, save_trust_root_records, TrustedRootRecord,
};
use custom_command_runtime::{run_custom_command_contract_runner, CustomCommandRuntimeConfig};
use dashboard_runtime::{run_dashboard_contract_runner, DashboardRuntimeConfig};
use deployment_runtime::{run_deployment_contract_runner, DeploymentRuntimeConfig};
use gateway_runtime::{run_gateway_contract_runner, GatewayRuntimeConfig};
use github_issues::{run_github_issues_bridge, GithubIssuesBridgeRuntimeConfig};
use memory_runtime::{run_memory_contract_runner, MemoryRuntimeConfig};
use multi_agent_runtime::{run_multi_agent_contract_runner, MultiAgentRuntimeConfig};
use multi_channel_runtime::{
    run_multi_channel_contract_runner, run_multi_channel_live_runner,
    MultiChannelLiveRuntimeConfig, MultiChannelRuntimeConfig,
};
use slack::{run_slack_bridge, SlackBridgeRuntimeConfig};
use voice_runtime::{run_voice_contract_runner, VoiceRuntimeConfig};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let cli = Cli::parse();
    run_cli(cli).await
}

#[cfg(test)]
mod tests;
