mod auth_commands;
mod bootstrap_helpers;
mod browser_automation_contract;
mod canvas;
mod channel_adapters;
mod channel_lifecycle;
mod channel_send;
mod channel_store;
mod channel_store_admin;
mod commands;
mod custom_command_contract;
mod dashboard_contract;
mod deployment_wasm;
mod events;
mod github_issues;
mod macro_profile_commands;
mod mcp_server;
mod memory_contract;
mod model_catalog;
mod multi_agent_router;
mod observability_loggers;
mod orchestrator_bridge;
mod project_index;
mod qa_loop_commands;
mod release_channel_commands;
mod rpc_capabilities;
mod rpc_protocol;
mod runtime_loop;
mod runtime_output;
mod runtime_types;
mod slack;
mod slack_helpers;
mod startup_dispatch;
mod startup_local_runtime;
mod startup_model_catalog;
mod startup_preflight;
mod startup_transport_modes;
mod tool_policy_config;
mod tools;
#[cfg(test)]
mod transport_conformance;
mod transport_health;
mod voice_contract;

use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use serde_json::Value;
use tau_agent_core::{Agent, AgentEvent};
#[cfg(test)]
pub(crate) use tau_ai::Provider;
use tau_ai::{LlmClient, Message, MessageRole, ModelRef};
pub(crate) use tau_extensions as extension_manifest;
pub(crate) use tau_skills as package_manifest;
#[cfg(test)]
pub(crate) use tau_skills as skills;
pub(crate) use tau_skills as skills_commands;

pub(crate) use crate::auth_commands::execute_auth_command;
#[cfg(test)]
pub(crate) use crate::auth_commands::{parse_auth_command, AuthCommand};
pub(crate) use crate::bootstrap_helpers::{command_file_error_mode_label, init_tracing};
pub(crate) use crate::canvas::{
    execute_canvas_command, CanvasCommandConfig, CanvasEventOrigin, CanvasSessionLinkContext,
    CANVAS_USAGE,
};
pub(crate) use crate::channel_store_admin::execute_channel_store_admin_command;
#[cfg(test)]
pub(crate) use crate::commands::handle_command;
pub(crate) use crate::commands::{
    execute_command_file, handle_command_with_session_import_mode, CommandAction, COMMAND_NAMES,
};
#[cfg(test)]
pub(crate) use crate::commands::{
    render_command_help, render_help_overview, unknown_command_message,
};
use crate::events::{
    execute_events_dry_run_command, execute_events_inspect_command,
    execute_events_simulate_command, execute_events_template_write_command,
    execute_events_validate_command, run_event_scheduler, EventSchedulerConfig,
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
    parse_models_list_args, render_model_show, render_models_list, ModelCatalog, MODELS_LIST_USAGE,
    MODEL_SHOW_USAGE,
};
pub(crate) use crate::multi_agent_router::{load_multi_agent_route_table, MultiAgentRouteTable};
#[cfg(test)]
pub(crate) use crate::observability_loggers::tool_audit_event_json;
pub(crate) use crate::observability_loggers::{PromptTelemetryLogger, ToolAuditLogger};
pub(crate) use crate::orchestrator_bridge::run_plan_first_prompt;
pub(crate) use crate::orchestrator_bridge::run_plan_first_prompt_with_policy_context;
pub(crate) use crate::orchestrator_bridge::run_plan_first_prompt_with_policy_context_and_routing;
pub(crate) use crate::package_manifest::{
    execute_package_activate_command, execute_package_activate_on_startup,
    execute_package_conflicts_command, execute_package_install_command,
    execute_package_list_command, execute_package_remove_command, execute_package_rollback_command,
    execute_package_show_command, execute_package_update_command, execute_package_validate_command,
};
pub(crate) use crate::project_index::execute_project_index_command;
pub(crate) use crate::qa_loop_commands::{
    execute_qa_loop_cli_command, execute_qa_loop_preflight_command, QA_LOOP_USAGE,
};
pub(crate) use crate::release_channel_commands::{
    default_release_channel_path, execute_release_channel_command, RELEASE_CHANNEL_USAGE,
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
    AuthCommandConfig, CommandExecutionContext, ProfileDefaults, RenderOptions,
    SkillsSyncCommandConfig,
};
#[cfg(test)]
pub(crate) use crate::runtime_types::{
    DoctorCommandConfig, DoctorMultiChannelReadinessConfig, DoctorProviderKeyStatus,
};
#[cfg(test)]
pub(crate) use crate::skills::default_skills_lock_path;
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
};
#[cfg(test)]
pub(crate) use crate::skills_commands::{
    render_skills_lock_write_success, render_skills_sync_drift_details,
};
use crate::startup_dispatch::run_cli;
#[cfg(test)]
pub(crate) use crate::startup_local_runtime::register_runtime_extension_tool_hook_subscriber;
pub(crate) use crate::startup_local_runtime::{run_local_runtime, LocalRuntimeConfig};
pub(crate) use crate::startup_model_catalog::{
    resolve_startup_model_catalog, validate_startup_model_catalog,
};
pub(crate) use crate::startup_preflight::execute_startup_preflight;
pub(crate) use crate::startup_transport_modes::run_transport_mode_if_requested;
pub(crate) use crate::tool_policy_config::build_tool_policy;
#[cfg(test)]
pub(crate) use crate::tool_policy_config::parse_sandbox_command_tokens;
#[cfg(test)]
pub(crate) use crate::tool_policy_config::tool_policy_to_json;
use crate::tools::ToolPolicy;
pub(crate) use crate::transport_health::TransportHealthSnapshot;
use github_issues::{run_github_issues_bridge, GithubIssuesBridgeRuntimeConfig};
use slack::{run_slack_bridge, SlackBridgeRuntimeConfig};
pub(crate) use tau_access::approvals::{
    evaluate_approval_gate, execute_approvals_command, ApprovalAction, ApprovalGateResult,
    APPROVALS_USAGE,
};
pub(crate) use tau_access::pairing::{
    evaluate_pairing_access, execute_pair_command, execute_unpair_command,
    pairing_policy_for_state_dir, PairingDecision,
};
pub(crate) use tau_access::rbac::{
    authorize_action_for_principal_with_policy_path, authorize_command_for_principal,
    execute_rbac_command, github_principal, rbac_policy_path_for_state_dir,
    resolve_local_principal, slack_principal, RbacDecision, RBAC_USAGE,
};
#[cfg(test)]
pub(crate) use tau_access::trust_roots::{
    load_trust_root_records, parse_trust_rotation_spec, parse_trusted_root_spec, TrustedRootRecord,
};
#[cfg(test)]
pub(crate) use tau_cli::parse_command;
#[cfg(test)]
pub(crate) use tau_cli::parse_command_file;
#[cfg(test)]
pub(crate) use tau_cli::validation::validate_gateway_remote_profile_inspect_cli;
#[cfg(test)]
pub(crate) use tau_cli::validation::validate_multi_channel_live_connectors_runner_cli;
#[cfg(test)]
pub(crate) use tau_cli::validation::{
    validate_custom_command_contract_runner_cli, validate_dashboard_contract_runner_cli,
    validate_deployment_contract_runner_cli, validate_events_runner_cli,
    validate_gateway_contract_runner_cli, validate_gateway_openresponses_server_cli,
    validate_github_issues_bridge_cli, validate_memory_contract_runner_cli,
    validate_multi_agent_contract_runner_cli, validate_multi_channel_contract_runner_cli,
    validate_multi_channel_live_runner_cli, validate_slack_bridge_cli,
    validate_voice_contract_runner_cli,
};
#[cfg(test)]
pub(crate) use tau_cli::validation::{
    validate_daemon_cli, validate_deployment_wasm_inspect_cli,
    validate_deployment_wasm_package_cli, validate_event_webhook_ingest_cli,
    validate_gateway_service_cli, validate_multi_channel_channel_lifecycle_cli,
    validate_multi_channel_incident_timeline_cli, validate_multi_channel_live_ingest_cli,
    validate_multi_channel_send_cli, validate_project_index_cli,
};
pub(crate) use tau_cli::Cli;
#[cfg(test)]
pub(crate) use tau_cli::CliProviderAuthMode;
#[cfg(test)]
pub(crate) use tau_cli::{
    CliBashProfile, CliCredentialStoreEncryptionMode, CliDeploymentWasmRuntimeProfile,
    CliGatewayOpenResponsesAuthMode, CliMultiChannelLiveConnectorMode, CliMultiChannelTransport,
    CliOsSandboxMode, CliSessionImportMode, CliToolPolicyPreset,
};
pub(crate) use tau_cli::{CliCommandFileErrorMode, CliEventTemplateSchedule, CliOrchestratorMode};
#[cfg(test)]
pub(crate) use tau_cli::{CliDaemonProfile, CliGatewayRemoteProfile};
#[cfg(test)]
pub(crate) use tau_cli::{CliMultiChannelOutboundMode, CliWebhookSignatureAlgorithm};
#[cfg(test)]
pub(crate) use tau_cli::{CommandFileEntry, CommandFileReport};
#[cfg(test)]
pub(crate) use tau_core::current_unix_timestamp;
pub(crate) use tau_core::current_unix_timestamp_ms;
pub(crate) use tau_core::write_text_atomic;
#[cfg(test)]
pub(crate) use tau_diagnostics::build_doctor_command_config;
#[cfg(test)]
pub(crate) use tau_diagnostics::{
    evaluate_multi_channel_live_readiness, parse_doctor_command_args, percentile_duration_ms,
    render_audit_summary, render_doctor_report, render_doctor_report_json, run_doctor_checks,
    run_doctor_checks_with_lookup, summarize_audit_file, DoctorCheckOptions, DoctorCheckResult,
    DoctorCommandArgs, DoctorCommandOutputFormat, DoctorStatus,
};
pub(crate) use tau_diagnostics::{
    execute_audit_summary_command, execute_browser_automation_preflight_command,
    execute_doctor_cli_command, execute_multi_channel_live_readiness_preflight_command,
    execute_policy_command,
};
#[cfg(test)]
pub(crate) use tau_diagnostics::{execute_doctor_command, execute_doctor_command_with_options};
#[cfg(test)]
pub(crate) use tau_multi_channel::build_multi_channel_incident_timeline_report;
#[cfg(test)]
pub(crate) use tau_multi_channel::build_multi_channel_route_inspect_report;
pub(crate) use tau_onboarding::onboarding_command::execute_onboarding_command;
pub(crate) use tau_onboarding::startup_config::build_auth_command_config;
#[cfg(test)]
pub(crate) use tau_onboarding::startup_config::build_profile_defaults;
pub(crate) use tau_onboarding::startup_model_resolution::{
    resolve_startup_models, StartupModelResolution,
};
#[cfg(test)]
pub(crate) use tau_onboarding::startup_prompt_composition::compose_startup_system_prompt;
#[cfg(test)]
pub(crate) use tau_onboarding::startup_resolution::apply_trust_root_mutations;
pub(crate) use tau_onboarding::startup_resolution::ensure_non_empty_text;
#[cfg(test)]
pub(crate) use tau_onboarding::startup_resolution::resolve_skill_trust_roots;
#[cfg(test)]
pub(crate) use tau_onboarding::startup_resolution::resolve_system_prompt;
pub(crate) use tau_onboarding::startup_skills_bootstrap::run_startup_skills_bootstrap;
#[cfg(test)]
pub(crate) use tau_orchestrator::parse_numbered_plan_steps;
#[cfg(test)]
pub(crate) use tau_provider::provider_auth_snapshot_for_status;
#[cfg(test)]
pub(crate) use tau_provider::refresh_provider_access_token;
#[cfg(test)]
pub(crate) use tau_provider::resolve_api_key;
#[cfg(test)]
pub(crate) use tau_provider::resolve_credential_store_encryption_mode;
#[cfg(test)]
pub(crate) use tau_provider::resolve_fallback_models;
#[cfg(test)]
pub(crate) use tau_provider::CredentialStoreEncryptionMode;
pub(crate) use tau_provider::{
    build_client_with_fallbacks, execute_integration_auth_command,
    resolve_secret_from_cli_or_store_id,
};
#[cfg(test)]
pub(crate) use tau_provider::{
    build_provider_client, decrypt_credential_store_secret, encrypt_credential_store_secret,
    provider_api_key_candidates_with_inputs, provider_auth_capability,
    IntegrationCredentialStoreRecord, ProviderAuthMethod,
};
#[cfg(test)]
pub(crate) use tau_provider::{
    is_retryable_provider_error, resolve_store_backed_provider_credential, ClientRoute,
    FallbackRoutingClient,
};
#[cfg(test)]
pub(crate) use tau_provider::{
    load_credential_store, save_credential_store, CredentialStoreData,
    ProviderCredentialStoreRecord,
};
#[cfg(test)]
pub(crate) use tau_provider::{parse_integration_auth_command, IntegrationAuthCommand};
pub(crate) use tau_session::execute_session_graph_export_command;
#[cfg(test)]
pub(crate) use tau_session::validate_session_file;
use tau_session::SessionImportMode;
#[cfg(test)]
pub(crate) use tau_session::{
    branch_alias_path_for_session, load_branch_aliases, load_session_bookmarks,
    parse_branch_alias_command, parse_session_bookmark_command, save_branch_aliases,
    save_session_bookmarks, session_bookmark_path_for_session, validate_branch_alias_name,
    BranchAliasCommand, BranchAliasFile, SessionBookmarkCommand, SessionBookmarkFile,
    BRANCH_ALIAS_SCHEMA_VERSION, BRANCH_ALIAS_USAGE, SESSION_BOOKMARK_SCHEMA_VERSION,
    SESSION_BOOKMARK_USAGE,
};
#[cfg(test)]
pub(crate) use tau_session::{
    compute_session_entry_depths, compute_session_stats, execute_session_diff_command,
    execute_session_search_command, execute_session_stats_command, parse_session_diff_args,
    parse_session_search_args, parse_session_stats_args, render_session_diff, render_session_stats,
    render_session_stats_json, search_session_entries, shared_lineage_prefix_depth,
    SessionDiffEntry, SessionDiffReport, SessionSearchArgs, SessionStats, SessionStatsOutputFormat,
    SESSION_SEARCH_DEFAULT_RESULTS, SESSION_SEARCH_PREVIEW_CHARS,
};
#[cfg(test)]
pub(crate) use tau_session::{
    escape_graph_label, render_session_graph_dot, render_session_graph_mermaid,
    resolve_session_graph_format, SessionGraphFormat,
};
pub(crate) use tau_session::{execute_branch_alias_command, execute_session_bookmark_command};
pub(crate) use tau_session::{
    format_id_list, format_remap_ids, initialize_session, session_lineage_messages, SessionRuntime,
};
pub(crate) use tau_session::{session_message_preview, session_message_role};

pub(crate) fn normalize_daemon_subcommand_args(args: Vec<String>) -> Vec<String> {
    if args.len() < 3 || args[1] != "daemon" {
        return args;
    }

    let action_flag = match args[2].as_str() {
        "install" => "--daemon-install",
        "uninstall" => "--daemon-uninstall",
        "start" => "--daemon-start",
        "stop" => "--daemon-stop",
        "status" => "--daemon-status",
        _ => return args,
    };

    let mut normalized = Vec::with_capacity(args.len());
    normalized.push(args[0].clone());
    normalized.push(action_flag.to_string());
    for argument in args.into_iter().skip(3) {
        match argument.as_str() {
            "--profile" => normalized.push("--daemon-profile".to_string()),
            "--state-dir" => normalized.push("--daemon-state-dir".to_string()),
            "--reason" => normalized.push("--daemon-stop-reason".to_string()),
            "--json" => normalized.push("--daemon-status-json".to_string()),
            other => normalized.push(other.to_string()),
        }
    }
    normalized
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let cli = Cli::parse_from(normalize_daemon_subcommand_args(std::env::args().collect()));
    run_cli(cli).await
}

#[cfg(test)]
mod tests;
