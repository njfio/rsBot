mod atomic_io;
mod auth_commands;
mod auth_types;
mod bootstrap_helpers;
mod channel_store;
mod channel_store_admin;
mod cli_args;
mod cli_types;
mod commands;
mod credentials;
mod diagnostics_commands;
mod events;
mod github_issues;
mod macro_profile_commands;
mod observability_loggers;
mod provider_auth;
mod provider_client;
mod provider_credentials;
mod provider_fallback;
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
mod startup_config;
mod startup_resolution;
mod time_utils;
mod tool_policy_config;
mod tools;
#[cfg(test)]
mod transport_conformance;
mod trust_roots;

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
use pi_agent_core::{Agent, AgentConfig, AgentEvent};
use pi_ai::{
    AnthropicClient, AnthropicConfig, ChatRequest, ChatResponse, GoogleClient, GoogleConfig,
    LlmClient, Message, MessageRole, ModelRef, OpenAiClient, OpenAiConfig, PiAiError, Provider,
    StreamDeltaHandler,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(crate) use crate::atomic_io::write_text_atomic;
pub(crate) use crate::auth_commands::execute_auth_command;
#[cfg(test)]
pub(crate) use crate::auth_commands::{parse_auth_command, AuthCommand};
pub(crate) use crate::auth_types::{CredentialStoreEncryptionMode, ProviderAuthMethod};
pub(crate) use crate::bootstrap_helpers::{command_file_error_mode_label, init_tracing};
use crate::channel_store::ChannelStore;
pub(crate) use crate::channel_store_admin::execute_channel_store_admin_command;
pub(crate) use crate::cli_args::Cli;
pub(crate) use crate::cli_types::{
    CliBashProfile, CliCommandFileErrorMode, CliCredentialStoreEncryptionMode, CliOsSandboxMode,
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
#[cfg(test)]
pub(crate) use crate::diagnostics_commands::execute_doctor_command;
pub(crate) use crate::diagnostics_commands::{
    build_doctor_command_config, execute_audit_summary_command, execute_doctor_cli_command,
    execute_policy_command,
};
#[cfg(test)]
pub(crate) use crate::diagnostics_commands::{
    parse_doctor_command_args, percentile_duration_ms, render_audit_summary, render_doctor_report,
    render_doctor_report_json, run_doctor_checks, summarize_audit_file, DoctorCheckResult,
    DoctorCommandOutputFormat, DoctorStatus,
};
use crate::events::{
    ingest_webhook_immediate_event, run_event_scheduler, EventSchedulerConfig,
    EventWebhookIngestConfig,
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
#[cfg(test)]
pub(crate) use crate::observability_loggers::tool_audit_event_json;
pub(crate) use crate::observability_loggers::{PromptTelemetryLogger, ToolAuditLogger};
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
pub(crate) use crate::runtime_cli_validation::{
    validate_event_webhook_ingest_cli, validate_events_runner_cli,
    validate_github_issues_bridge_cli, validate_slack_bridge_cli,
};
pub(crate) use crate::runtime_loop::{
    resolve_prompt_input, run_interactive, run_prompt, run_prompt_with_cancellation,
    InteractiveRuntimeConfig, PromptRunStatus,
};
#[cfg(test)]
pub(crate) use crate::runtime_output::stream_text_chunks;
pub(crate) use crate::runtime_output::{
    event_to_json, persist_messages, print_assistant_messages, summarize_message,
};
pub(crate) use crate::runtime_types::{
    AuthCommandConfig, CommandExecutionContext, DoctorCommandConfig, DoctorProviderKeyStatus,
    ProfileAuthDefaults, ProfileDefaults, ProfilePolicyDefaults, ProfileSessionDefaults,
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
pub(crate) use crate::startup_resolution::{
    ensure_non_empty_text, resolve_skill_trust_roots, resolve_system_prompt,
};
pub(crate) use crate::time_utils::{
    current_unix_timestamp, current_unix_timestamp_ms, is_expired_unix,
};
#[cfg(test)]
pub(crate) use crate::tool_policy_config::parse_sandbox_command_tokens;
pub(crate) use crate::tool_policy_config::{build_tool_policy, tool_policy_to_json};
use crate::tools::{tool_policy_preset_name, ToolPolicy};
pub(crate) use crate::trust_roots::{
    apply_trust_root_mutation_specs, apply_trust_root_mutations, load_trust_root_records,
    parse_trust_rotation_spec, parse_trusted_root_spec, save_trust_root_records, TrustedRootRecord,
};
use github_issues::{run_github_issues_bridge, GithubIssuesBridgeRuntimeConfig};
use slack::{run_slack_bridge, SlackBridgeRuntimeConfig};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let cli = Cli::parse();

    if cli.session_validate {
        validate_session_file(&cli)?;
        return Ok(());
    }
    if cli.channel_store_inspect.is_some() || cli.channel_store_repair.is_some() {
        execute_channel_store_admin_command(&cli)?;
        return Ok(());
    }
    if cli.event_webhook_ingest_file.is_some() {
        validate_event_webhook_ingest_cli(&cli)?;
        let payload_file = cli
            .event_webhook_ingest_file
            .clone()
            .ok_or_else(|| anyhow!("--event-webhook-ingest-file is required"))?;
        let channel_ref = cli
            .event_webhook_channel
            .clone()
            .ok_or_else(|| anyhow!("--event-webhook-channel is required"))?;
        let event_webhook_secret = resolve_secret_from_cli_or_store_id(
            &cli,
            cli.event_webhook_secret.as_deref(),
            cli.event_webhook_secret_id.as_deref(),
            "--event-webhook-secret-id",
        )?;
        ingest_webhook_immediate_event(&EventWebhookIngestConfig {
            events_dir: cli.events_dir.clone(),
            state_path: cli.events_state_path.clone(),
            channel_ref,
            payload_file,
            prompt_prefix: cli.event_webhook_prompt_prefix.clone(),
            debounce_key: cli.event_webhook_debounce_key.clone(),
            debounce_window_seconds: cli.event_webhook_debounce_window_seconds,
            signature: cli.event_webhook_signature.clone(),
            timestamp: cli.event_webhook_timestamp.clone(),
            secret: event_webhook_secret,
            signature_algorithm: cli.event_webhook_signature_algorithm.map(Into::into),
            signature_max_skew_seconds: cli.event_webhook_signature_max_skew_seconds,
        })?;
        return Ok(());
    }

    if cli.no_session && cli.branch_from.is_some() {
        bail!("--branch-from cannot be used together with --no-session");
    }

    let model_ref = ModelRef::parse(&cli.model)
        .map_err(|error| anyhow!("failed to parse --model '{}': {error}", cli.model))?;
    let fallback_model_refs = resolve_fallback_models(&cli, &model_ref)?;

    let client = build_client_with_fallbacks(&cli, &model_ref, &fallback_model_refs)?;
    let mut skill_lock_hints = Vec::new();
    if !cli.install_skill.is_empty() {
        let report = install_skills(&cli.install_skill, &cli.skills_dir)?;
        skill_lock_hints.extend(build_local_skill_lock_hints(&cli.install_skill)?);
        println!(
            "skills install: installed={} updated={} skipped={}",
            report.installed, report.updated, report.skipped
        );
    }
    let skills_download_options = SkillsDownloadOptions {
        cache_dir: Some(
            cli.skills_cache_dir
                .clone()
                .unwrap_or_else(|| default_skills_cache_dir(&cli.skills_dir)),
        ),
        offline: cli.skills_offline,
    };
    let remote_skill_sources =
        resolve_remote_skill_sources(&cli.install_skill_url, &cli.install_skill_sha256)?;
    if !remote_skill_sources.is_empty() {
        let report = install_remote_skills_with_cache(
            &remote_skill_sources,
            &cli.skills_dir,
            &skills_download_options,
        )
        .await?;
        skill_lock_hints.extend(build_remote_skill_lock_hints(&remote_skill_sources)?);
        println!(
            "remote skills install: installed={} updated={} skipped={}",
            report.installed, report.updated, report.skipped
        );
    }
    let trusted_skill_roots = resolve_skill_trust_roots(&cli)?;
    if !cli.install_skill_from_registry.is_empty() {
        let registry_url = cli.skill_registry_url.as_deref().ok_or_else(|| {
            anyhow!("--skill-registry-url is required when using --install-skill-from-registry")
        })?;
        let manifest = fetch_registry_manifest_with_cache(
            registry_url,
            cli.skill_registry_sha256.as_deref(),
            &skills_download_options,
        )
        .await?;
        let sources = resolve_registry_skill_sources(
            &manifest,
            &cli.install_skill_from_registry,
            &trusted_skill_roots,
            cli.require_signed_skills,
        )?;
        let report =
            install_remote_skills_with_cache(&sources, &cli.skills_dir, &skills_download_options)
                .await?;
        skill_lock_hints.extend(build_registry_skill_lock_hints(
            registry_url,
            &cli.install_skill_from_registry,
            &sources,
        )?);
        println!(
            "registry skills install: installed={} updated={} skipped={}",
            report.installed, report.updated, report.skipped
        );
    }
    let skills_lock_path = cli
        .skills_lock_file
        .clone()
        .unwrap_or_else(|| default_skills_lock_path(&cli.skills_dir));
    if cli.skills_lock_write {
        let lockfile =
            write_skills_lockfile(&cli.skills_dir, &skills_lock_path, &skill_lock_hints)?;
        println!(
            "{}",
            render_skills_lock_write_success(&skills_lock_path, lockfile.entries.len())
        );
    }
    if cli.skills_sync {
        let report = sync_skills_with_lockfile(&cli.skills_dir, &skills_lock_path)?;
        if report.in_sync() {
            println!("{}", render_skills_sync_in_sync(&skills_lock_path, &report));
        } else {
            bail!(
                "skills sync drift detected: path={} {}",
                skills_lock_path.display(),
                render_skills_sync_drift_details(&report)
            );
        }
    }
    let base_system_prompt = resolve_system_prompt(&cli)?;
    let catalog = load_catalog(&cli.skills_dir)
        .with_context(|| format!("failed to load skills from {}", cli.skills_dir.display()))?;
    let selected_skills = resolve_selected_skills(&catalog, &cli.skills)?;
    let system_prompt = augment_system_prompt(&base_system_prompt, &selected_skills);

    let tool_policy = build_tool_policy(&cli)?;
    let tool_policy_json = tool_policy_to_json(&tool_policy);
    if cli.print_tool_policy {
        println!("{tool_policy_json}");
    }
    let render_options = RenderOptions::from_cli(&cli);
    validate_github_issues_bridge_cli(&cli)?;
    validate_slack_bridge_cli(&cli)?;
    validate_events_runner_cli(&cli)?;
    if cli.github_issues_bridge {
        let repo_slug = cli.github_repo.clone().ok_or_else(|| {
            anyhow!("--github-repo is required when --github-issues-bridge is set")
        })?;
        let token = resolve_secret_from_cli_or_store_id(
            &cli,
            cli.github_token.as_deref(),
            cli.github_token_id.as_deref(),
            "--github-token-id",
        )?
        .ok_or_else(|| {
            anyhow!(
                "--github-token (or --github-token-id) is required when --github-issues-bridge is set"
            )
        })?;
        return run_github_issues_bridge(GithubIssuesBridgeRuntimeConfig {
            client: client.clone(),
            model: model_ref.model.clone(),
            system_prompt: system_prompt.clone(),
            max_turns: cli.max_turns,
            tool_policy: tool_policy.clone(),
            turn_timeout_ms: cli.turn_timeout_ms,
            request_timeout_ms: cli.request_timeout_ms,
            render_options,
            session_lock_wait_ms: cli.session_lock_wait_ms,
            session_lock_stale_ms: cli.session_lock_stale_ms,
            state_dir: cli.github_state_dir.clone(),
            repo_slug,
            api_base: cli.github_api_base.clone(),
            token,
            bot_login: cli.github_bot_login.clone(),
            poll_interval: Duration::from_secs(cli.github_poll_interval_seconds.max(1)),
            include_issue_body: cli.github_include_issue_body,
            include_edited_comments: cli.github_include_edited_comments,
            processed_event_cap: cli.github_processed_event_cap.max(1),
            retry_max_attempts: cli.github_retry_max_attempts.max(1),
            retry_base_delay_ms: cli.github_retry_base_delay_ms.max(1),
        })
        .await;
    }
    if cli.slack_bridge {
        let app_token = resolve_secret_from_cli_or_store_id(
            &cli,
            cli.slack_app_token.as_deref(),
            cli.slack_app_token_id.as_deref(),
            "--slack-app-token-id",
        )?
        .ok_or_else(|| {
            anyhow!("--slack-app-token (or --slack-app-token-id) is required when --slack-bridge is set")
        })?;
        let bot_token = resolve_secret_from_cli_or_store_id(
            &cli,
            cli.slack_bot_token.as_deref(),
            cli.slack_bot_token_id.as_deref(),
            "--slack-bot-token-id",
        )?
        .ok_or_else(|| {
            anyhow!("--slack-bot-token (or --slack-bot-token-id) is required when --slack-bridge is set")
        })?;
        return run_slack_bridge(SlackBridgeRuntimeConfig {
            client: client.clone(),
            model: model_ref.model.clone(),
            system_prompt: system_prompt.clone(),
            max_turns: cli.max_turns,
            tool_policy: tool_policy.clone(),
            turn_timeout_ms: cli.turn_timeout_ms,
            request_timeout_ms: cli.request_timeout_ms,
            render_options,
            session_lock_wait_ms: cli.session_lock_wait_ms,
            session_lock_stale_ms: cli.session_lock_stale_ms,
            state_dir: cli.slack_state_dir.clone(),
            api_base: cli.slack_api_base.clone(),
            app_token,
            bot_token,
            bot_user_id: cli.slack_bot_user_id.clone(),
            detail_thread_output: cli.slack_thread_detail_output,
            detail_thread_threshold_chars: cli.slack_thread_detail_threshold_chars.max(1),
            processed_event_cap: cli.slack_processed_event_cap.max(1),
            max_event_age_seconds: cli.slack_max_event_age_seconds,
            reconnect_delay: Duration::from_millis(cli.slack_reconnect_delay_ms.max(1)),
            retry_max_attempts: cli.slack_retry_max_attempts.max(1),
            retry_base_delay_ms: cli.slack_retry_base_delay_ms.max(1),
        })
        .await;
    }
    if cli.events_runner {
        return run_event_scheduler(EventSchedulerConfig {
            client: client.clone(),
            model: model_ref.model.clone(),
            system_prompt: system_prompt.clone(),
            max_turns: cli.max_turns,
            tool_policy: tool_policy.clone(),
            turn_timeout_ms: cli.turn_timeout_ms,
            render_options,
            session_lock_wait_ms: cli.session_lock_wait_ms,
            session_lock_stale_ms: cli.session_lock_stale_ms,
            channel_store_root: cli.channel_store_root.clone(),
            events_dir: cli.events_dir.clone(),
            state_path: cli.events_state_path.clone(),
            poll_interval: Duration::from_millis(cli.events_poll_interval_ms.max(1)),
            queue_limit: cli.events_queue_limit.max(1),
            stale_immediate_max_age_seconds: cli.events_stale_immediate_max_age_seconds,
        })
        .await;
    }

    let mut agent = Agent::new(
        client.clone(),
        AgentConfig {
            model: model_ref.model.clone(),
            system_prompt: system_prompt.clone(),
            max_turns: cli.max_turns,
            temperature: Some(0.0),
            max_tokens: None,
        },
    );
    tools::register_builtin_tools(&mut agent, tool_policy);
    if let Some(path) = cli.tool_audit_log.clone() {
        let logger = ToolAuditLogger::open(path)?;
        agent.subscribe(move |event| {
            if let Err(error) = logger.log_event(event) {
                eprintln!("tool audit logger error: {error}");
            }
        });
    }
    if let Some(path) = cli.telemetry_log.clone() {
        let logger =
            PromptTelemetryLogger::open(path, model_ref.provider.as_str(), &model_ref.model)?;
        agent.subscribe(move |event| {
            if let Err(error) = logger.log_event(event) {
                eprintln!("telemetry logger error: {error}");
            }
        });
    }
    let mut session_runtime = if cli.no_session {
        None
    } else {
        Some(initialize_session(&mut agent, &cli, &system_prompt)?)
    };

    if cli.json_events {
        agent.subscribe(|event| {
            let value = event_to_json(event);
            println!("{value}");
        });
    }

    if let Some(prompt) = resolve_prompt_input(&cli)? {
        run_prompt(
            &mut agent,
            &mut session_runtime,
            &prompt,
            cli.turn_timeout_ms,
            render_options,
        )
        .await?;
        return Ok(());
    }

    let skills_sync_command_config = SkillsSyncCommandConfig {
        skills_dir: cli.skills_dir.clone(),
        default_lock_path: skills_lock_path.clone(),
        default_trust_root_path: cli.skill_trust_root_file.clone(),
        doctor_config: build_doctor_command_config(
            &cli,
            &model_ref,
            &fallback_model_refs,
            &skills_lock_path,
        ),
    };
    let profile_defaults = build_profile_defaults(&cli);
    let auth_command_config = build_auth_command_config(&cli);
    let command_context = CommandExecutionContext {
        tool_policy_json: &tool_policy_json,
        session_import_mode: cli.session_import_mode.into(),
        profile_defaults: &profile_defaults,
        skills_command_config: &skills_sync_command_config,
        auth_command_config: &auth_command_config,
    };
    let interactive_config = InteractiveRuntimeConfig {
        turn_timeout_ms: cli.turn_timeout_ms,
        render_options,
        command_context,
    };
    if let Some(command_file_path) = cli.command_file.as_deref() {
        execute_command_file(
            command_file_path,
            cli.command_file_error_mode,
            &mut agent,
            &mut session_runtime,
            command_context,
        )?;
        return Ok(());
    }
    run_interactive(agent, session_runtime, interactive_config).await
}

#[cfg(test)]
mod tests;
