//! Local runtime bootstrap from resolved startup context.
//!
//! This module builds local runtime dependencies and entry-mode execution paths
//! after preflight/model/policy resolution completes.

use std::{path::Path, sync::Arc};

use anyhow::Result;
use serde_json::Value;
#[cfg(test)]
use tau_agent_core::Agent;
use tau_agent_core::{AgentConfig, AgentEvent, SafetyMode};
use tau_ai::{LlmClient, ModelRef};
use tau_cli::{Cli, CliPromptSanitizerMode};
use tau_runtime::start_runtime_heartbeat_scheduler;
use tau_session::initialize_session;

use crate::commands::execute_command_file;
use crate::extension_manifest::{
    discover_extension_runtime_registrations, dispatch_extension_runtime_hook,
    ExtensionRuntimeRegistrationSummary,
};
use crate::mcp_client::register_mcp_client_tools;
use crate::model_catalog::ModelCatalog;
use crate::multi_agent_router::load_multi_agent_route_table;
use crate::observability_loggers::{PromptTelemetryLogger, ToolAuditLogger};
use crate::runtime_loop::{
    resolve_prompt_input, run_interactive, run_plan_first_prompt_with_runtime_hooks, run_prompt,
    InteractiveRuntimeConfig, PlanFirstPromptRuntimeHooksConfig, RuntimeExtensionHooksConfig,
};
use crate::runtime_output::event_to_json;
use crate::runtime_types::{CommandExecutionContext, RenderOptions, SkillsSyncCommandConfig};
use crate::tools::{self, ToolPolicy};
use tau_onboarding::startup_local_runtime::{
    build_local_runtime_agent as build_onboarding_local_runtime_agent,
    build_local_runtime_extension_startup as build_onboarding_local_runtime_extension_startup,
    execute_local_runtime_entry_mode_with_dispatch as execute_onboarding_local_runtime_entry_mode_with_dispatch,
    register_runtime_extension_pipeline as register_onboarding_runtime_extension_pipeline,
    register_runtime_observability_if_configured as register_onboarding_runtime_observability_if_configured,
    resolve_local_runtime_startup_from_cli as resolve_onboarding_local_runtime_startup_from_cli,
    resolve_session_runtime_from_cli as resolve_onboarding_session_runtime_from_cli,
    LocalRuntimeAgentSettings, LocalRuntimeCommandDefaults, LocalRuntimeEntryDispatch,
    LocalRuntimeExtensionBootstrap, LocalRuntimeExtensionStartup, LocalRuntimeStartupResolution,
    RuntimeEventReporterRegistrationConfig,
    RuntimeExtensionPipelineConfig as OnboardingRuntimeExtensionPipelineConfig,
    SessionBootstrapOutcome,
};
use tau_onboarding::startup_transport_modes::build_runtime_heartbeat_scheduler_config as build_onboarding_runtime_heartbeat_scheduler_config;

pub(crate) struct LocalRuntimeConfig<'a> {
    pub(crate) cli: &'a Cli,
    pub(crate) client: Arc<dyn LlmClient>,
    pub(crate) model_ref: &'a ModelRef,
    pub(crate) fallback_model_refs: &'a [ModelRef],
    pub(crate) model_catalog: &'a ModelCatalog,
    pub(crate) system_prompt: &'a str,
    pub(crate) tool_policy: ToolPolicy,
    pub(crate) tool_policy_json: &'a Value,
    pub(crate) render_options: RenderOptions,
    pub(crate) skills_dir: &'a Path,
    pub(crate) skills_lock_path: &'a Path,
}

fn resolve_safety_mode(mode: CliPromptSanitizerMode) -> SafetyMode {
    match mode {
        CliPromptSanitizerMode::Warn => SafetyMode::Warn,
        CliPromptSanitizerMode::Redact => SafetyMode::Redact,
        CliPromptSanitizerMode::Block => SafetyMode::Block,
    }
}

pub(crate) async fn run_local_runtime(config: LocalRuntimeConfig<'_>) -> Result<()> {
    let LocalRuntimeConfig {
        cli,
        client,
        model_ref,
        fallback_model_refs,
        model_catalog,
        system_prompt,
        tool_policy,
        tool_policy_json,
        render_options,
        skills_dir,
        skills_lock_path,
    } = config;

    let agent_defaults = AgentConfig::default();
    let model_catalog_entry = model_catalog.find_model_ref(model_ref);
    let mut agent = build_onboarding_local_runtime_agent(
        client,
        model_ref,
        system_prompt,
        LocalRuntimeAgentSettings {
            max_turns: cli.max_turns,
            max_parallel_tool_calls: cli.agent_max_parallel_tool_calls,
            max_context_messages: cli.agent_max_context_messages,
            request_max_retries: cli.agent_request_max_retries,
            request_retry_initial_backoff_ms: cli.agent_request_retry_initial_backoff_ms,
            request_retry_max_backoff_ms: cli.agent_request_retry_max_backoff_ms,
            request_timeout_ms: agent_defaults.request_timeout_ms,
            tool_timeout_ms: agent_defaults.tool_timeout_ms,
            model_input_cost_per_million: model_catalog_entry
                .and_then(|entry| entry.input_cost_per_million),
            model_output_cost_per_million: model_catalog_entry
                .and_then(|entry| entry.output_cost_per_million),
            cost_budget_usd: cli.agent_cost_budget_usd,
            cost_alert_thresholds_percent: cli.agent_cost_alert_threshold_percent.clone(),
            prompt_sanitizer_enabled: cli.prompt_sanitizer_enabled,
            prompt_sanitizer_mode: resolve_safety_mode(cli.prompt_sanitizer_mode),
            prompt_sanitizer_redaction_token: cli.prompt_sanitizer_redaction_token.clone(),
            secret_leak_detector_enabled: cli.secret_leak_detector_enabled,
            secret_leak_detector_mode: resolve_safety_mode(cli.secret_leak_detector_mode),
            secret_leak_redaction_token: cli.secret_leak_redaction_token.clone(),
        },
        tool_policy,
    );
    register_onboarding_runtime_observability_if_configured(
        &mut agent,
        RuntimeEventReporterRegistrationConfig {
            path: cli.tool_audit_log.clone(),
            open_reporter: ToolAuditLogger::open,
            report_event: |logger: &ToolAuditLogger, event: &AgentEvent| logger.log_event(event),
            emit_error: |error: &str| eprintln!("tool audit logger error: {error}"),
        },
        RuntimeEventReporterRegistrationConfig {
            path: cli.telemetry_log.clone(),
            open_reporter: |path| {
                PromptTelemetryLogger::open(path, model_ref.provider.as_str(), &model_ref.model)
            },
            report_event: |logger: &PromptTelemetryLogger, event: &AgentEvent| {
                logger.log_event(event)
            },
            emit_error: |error: &str| eprintln!("telemetry logger error: {error}"),
        },
        cli.json_events,
        event_to_json,
        |value| println!("{value}"),
    )?;
    let mut session_runtime = resolve_onboarding_session_runtime_from_cli(
        cli,
        system_prompt,
        |session_path, lock_wait_ms, lock_stale_ms, branch_from, prompt| {
            let outcome = initialize_session(
                session_path,
                lock_wait_ms,
                lock_stale_ms,
                branch_from,
                prompt,
            )?;
            Ok(SessionBootstrapOutcome {
                runtime: outcome.runtime,
                lineage: outcome.lineage,
            })
        },
        |lineage| agent.replace_messages(lineage),
    )?;
    let LocalRuntimeExtensionStartup {
        extension_hooks,
        bootstrap:
            LocalRuntimeExtensionBootstrap {
                orchestrator_route_table,
                orchestrator_route_trace_log,
                extension_runtime_registrations,
            },
    } = build_onboarding_local_runtime_extension_startup(
        cli,
        load_multi_agent_route_table,
        |root| {
            discover_extension_runtime_registrations(
                root,
                tools::builtin_agent_tool_names(),
                crate::commands::COMMAND_NAMES,
            )
        },
        |root| ExtensionRuntimeRegistrationSummary {
            root: root.to_path_buf(),
            discovered: 0,
            registered_tools: Vec::new(),
            registered_commands: Vec::new(),
            skipped_invalid: 0,
            skipped_unsupported_runtime: 0,
            skipped_permission_denied: 0,
            skipped_name_conflict: 0,
            diagnostics: Vec::new(),
        },
    )?;
    let extension_runtime_hooks = RuntimeExtensionHooksConfig {
        enabled: extension_hooks.enabled,
        root: extension_hooks.root.clone(),
    };
    let orchestrator_route_trace_log = orchestrator_route_trace_log.as_deref();
    register_onboarding_runtime_extension_pipeline(
        &mut agent,
        OnboardingRuntimeExtensionPipelineConfig {
            enabled: extension_hooks.enabled,
            root: extension_hooks.root,
            registered_tools: &extension_runtime_registrations.registered_tools,
            diagnostics: &extension_runtime_registrations.diagnostics,
        },
        tools::register_extension_tools,
        |diagnostic| eprintln!("{diagnostic}"),
        |root, hook, payload| dispatch_extension_runtime_hook(root, hook, payload).diagnostics,
    );
    let mcp_registration = register_mcp_client_tools(&mut agent, cli)?;
    if cli.mcp_client {
        eprintln!(
            "mcp client registration: config={} servers={} discovered_tools={} registered_tools={}",
            mcp_registration.config_path,
            mcp_registration.server_count,
            mcp_registration.discovered_tool_count,
            mcp_registration.registered_tool_count
        );
        for diagnostic in &mcp_registration.diagnostics {
            eprintln!(
                "mcp client diagnostic: server={} phase={} status={} reason_code={} detail={}",
                diagnostic.server,
                diagnostic.phase,
                diagnostic.status,
                diagnostic.reason_code,
                diagnostic.detail
            );
        }
    }

    let LocalRuntimeStartupResolution {
        interactive_defaults,
        entry_mode,
        command_defaults,
    } = resolve_onboarding_local_runtime_startup_from_cli(
        cli,
        model_ref,
        fallback_model_refs,
        skills_dir,
        skills_lock_path,
        resolve_prompt_input,
    )?;
    let LocalRuntimeCommandDefaults {
        profile_defaults,
        auth_command_config,
        doctor_config,
    } = command_defaults;
    let skills_sync_command_config = SkillsSyncCommandConfig {
        skills_dir: skills_dir.to_path_buf(),
        default_lock_path: skills_lock_path.to_path_buf(),
        default_trust_root_path: cli.skill_trust_root_file.clone(),
        doctor_config,
    };
    let command_context = CommandExecutionContext {
        tool_policy_json,
        session_import_mode: cli.session_import_mode.into(),
        profile_defaults: &profile_defaults,
        skills_command_config: &skills_sync_command_config,
        auth_command_config: &auth_command_config,
        model_catalog,
        extension_commands: &extension_runtime_registrations.registered_commands,
    };
    let interactive_config = InteractiveRuntimeConfig {
        turn_timeout_ms: interactive_defaults.turn_timeout_ms,
        render_options,
        extension_runtime_hooks: &extension_runtime_hooks,
        orchestrator_mode: interactive_defaults.orchestrator_mode,
        orchestrator_max_plan_steps: interactive_defaults.orchestrator_max_plan_steps,
        orchestrator_max_delegated_steps: interactive_defaults.orchestrator_max_delegated_steps,
        orchestrator_max_executor_response_chars: interactive_defaults
            .orchestrator_max_executor_response_chars,
        orchestrator_max_delegated_step_response_chars: interactive_defaults
            .orchestrator_max_delegated_step_response_chars,
        orchestrator_max_delegated_total_response_chars: interactive_defaults
            .orchestrator_max_delegated_total_response_chars,
        orchestrator_delegate_steps: interactive_defaults.orchestrator_delegate_steps,
        orchestrator_route_table: &orchestrator_route_table,
        orchestrator_route_trace_log,
        command_context,
    };

    let mut runtime_heartbeat_handle = start_runtime_heartbeat_scheduler(
        build_onboarding_runtime_heartbeat_scheduler_config(cli),
    )?;
    let run_result = async {
        if execute_onboarding_local_runtime_entry_mode_with_dispatch(
            &entry_mode,
            |entry_dispatch| async {
                match entry_dispatch {
                    LocalRuntimeEntryDispatch::PlanFirstPrompt(prompt) => {
                        run_plan_first_prompt_with_runtime_hooks(
                            &mut agent,
                            &mut session_runtime,
                            PlanFirstPromptRuntimeHooksConfig {
                                prompt: &prompt,
                                turn_timeout_ms: interactive_defaults.turn_timeout_ms,
                                render_options,
                                orchestrator_max_plan_steps: interactive_defaults
                                    .orchestrator_max_plan_steps,
                                orchestrator_max_delegated_steps: interactive_defaults
                                    .orchestrator_max_delegated_steps,
                                orchestrator_max_executor_response_chars: interactive_defaults
                                    .orchestrator_max_executor_response_chars,
                                orchestrator_max_delegated_step_response_chars:
                                    interactive_defaults
                                        .orchestrator_max_delegated_step_response_chars,
                                orchestrator_max_delegated_total_response_chars:
                                    interactive_defaults
                                        .orchestrator_max_delegated_total_response_chars,
                                orchestrator_delegate_steps: interactive_defaults
                                    .orchestrator_delegate_steps,
                                orchestrator_route_table: &orchestrator_route_table,
                                orchestrator_route_trace_log,
                                tool_policy_json,
                                extension_runtime_hooks: &extension_runtime_hooks,
                            },
                        )
                        .await?;
                    }
                    LocalRuntimeEntryDispatch::Prompt(prompt) => {
                        run_prompt(
                            &mut agent,
                            &mut session_runtime,
                            &prompt,
                            interactive_defaults.turn_timeout_ms,
                            render_options,
                            &extension_runtime_hooks,
                        )
                        .await?;
                    }
                    LocalRuntimeEntryDispatch::CommandFile(command_file_path) => {
                        execute_command_file(
                            &command_file_path,
                            cli.command_file_error_mode,
                            &mut agent,
                            &mut session_runtime,
                            command_context,
                        )?;
                    }
                }
                Ok(())
            },
        )
        .await?
        {
            return Ok(());
        }

        run_interactive(agent, session_runtime, interactive_config).await
    }
    .await;
    runtime_heartbeat_handle.shutdown().await;
    run_result
}

#[cfg(test)]
pub(crate) fn register_runtime_extension_tool_hook_subscriber(
    agent: &mut Agent,
    extension_runtime_hooks: &RuntimeExtensionHooksConfig,
) {
    tau_onboarding::startup_local_runtime::register_runtime_extension_tool_hook_subscriber(
        agent,
        extension_runtime_hooks.enabled,
        extension_runtime_hooks.root.clone(),
        |root, hook, payload| dispatch_extension_runtime_hook(root, hook, payload).diagnostics,
    );
}
