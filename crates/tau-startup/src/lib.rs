//! Startup preflight and runtime wiring entrypoints for Tau.
//!
//! Provides startup policy/transport dispatch helpers, command-file execution,
//! and startup-time runtime composition primitives.

use anyhow::{anyhow, Context, Result};
use tau_access::pairing::{evaluate_pairing_access, pairing_policy_for_state_dir, PairingDecision};
use tau_cli::validation::{
    validate_browser_automation_preflight_cli, validate_deployment_wasm_inspect_cli,
    validate_deployment_wasm_package_cli, validate_event_webhook_ingest_cli,
    validate_gateway_remote_plan_cli, validate_gateway_remote_profile_inspect_cli,
    validate_gateway_service_cli, validate_multi_channel_channel_lifecycle_cli,
    validate_multi_channel_incident_timeline_cli, validate_multi_channel_live_ingest_cli,
    validate_multi_channel_send_cli, validate_project_index_cli,
};
use tau_cli::Cli;
use tau_core::current_unix_timestamp_ms;
use tau_events::{
    ingest_webhook_immediate_event, EventWebhookIngestConfig, WebhookSignatureAlgorithm,
};
use tau_gateway::{
    inspect_gateway_service_mode, render_gateway_service_status_report, start_gateway_service_mode,
    stop_gateway_service_mode,
};
use tau_session::validate_session_file;

/// Shared startup/runtime value objects used across CLI entrypoints.
pub mod runtime_types;
/// Command-file execution runtime helpers.
pub mod startup_command_file_runtime;
/// Model-catalog resolution and validation helpers for startup.
pub mod startup_model_catalog;
/// Multi-channel adapter factories used by startup command wiring.
pub mod startup_multi_channel_adapters;
/// Multi-channel command implementations executed during startup preflight.
pub mod startup_multi_channel_commands;
/// RPC capabilities command rendering and dispatch helpers.
pub mod startup_rpc_capabilities_command;

pub use runtime_types::*;
pub use startup_command_file_runtime::*;
pub use startup_model_catalog::*;
pub use startup_multi_channel_adapters::*;
pub use startup_multi_channel_commands::*;
pub use startup_rpc_capabilities_command::*;

/// Trait contract for `StartupPreflightActions` behavior.
pub trait StartupPreflightActions {
    fn execute_onboarding_command(&self, cli: &Cli) -> Result<()>;
    fn execute_multi_channel_send_command(&self, cli: &Cli) -> Result<()>;
    fn execute_multi_channel_channel_lifecycle_command(&self, cli: &Cli) -> Result<()>;
    fn execute_deployment_wasm_package_command(&self, cli: &Cli) -> Result<()>;
    fn execute_deployment_wasm_inspect_command(&self, cli: &Cli) -> Result<()>;
    fn execute_project_index_command(&self, cli: &Cli) -> Result<()>;
    fn execute_channel_store_admin_command(&self, cli: &Cli) -> Result<()>;
    fn execute_multi_channel_live_readiness_preflight_command(&self, cli: &Cli) -> Result<()>;
    fn execute_browser_automation_preflight_command(&self, cli: &Cli) -> Result<()>;
    fn execute_extension_exec_command(&self, cli: &Cli) -> Result<()>;
    fn execute_extension_list_command(&self, cli: &Cli) -> Result<()>;
    fn execute_extension_show_command(&self, cli: &Cli) -> Result<()>;
    fn execute_extension_validate_command(&self, cli: &Cli) -> Result<()>;
    fn execute_package_validate_command(&self, cli: &Cli) -> Result<()>;
    fn execute_package_show_command(&self, cli: &Cli) -> Result<()>;
    fn execute_package_install_command(&self, cli: &Cli) -> Result<()>;
    fn execute_package_update_command(&self, cli: &Cli) -> Result<()>;
    fn execute_package_list_command(&self, cli: &Cli) -> Result<()>;
    fn execute_package_remove_command(&self, cli: &Cli) -> Result<()>;
    fn execute_package_rollback_command(&self, cli: &Cli) -> Result<()>;
    fn execute_package_conflicts_command(&self, cli: &Cli) -> Result<()>;
    fn execute_package_activate_command(&self, cli: &Cli) -> Result<()>;
    fn execute_qa_loop_preflight_command(&self, cli: &Cli) -> Result<()>;
    fn execute_mcp_server_command(&self, cli: &Cli) -> Result<()>;
    fn execute_rpc_capabilities_command(&self, cli: &Cli) -> Result<()>;
    fn execute_rpc_validate_frame_command(&self, cli: &Cli) -> Result<()>;
    fn execute_rpc_dispatch_frame_command(&self, cli: &Cli) -> Result<()>;
    fn execute_rpc_dispatch_ndjson_command(&self, cli: &Cli) -> Result<()>;
    fn execute_rpc_serve_ndjson_command(&self, cli: &Cli) -> Result<()>;
    fn execute_events_inspect_command(&self, cli: &Cli) -> Result<()>;
    fn execute_events_validate_command(&self, cli: &Cli) -> Result<()>;
    fn execute_events_simulate_command(&self, cli: &Cli) -> Result<()>;
    fn execute_events_dry_run_command(&self, cli: &Cli) -> Result<()>;
    fn execute_events_template_write_command(&self, cli: &Cli) -> Result<()>;
    fn resolve_secret_from_cli_or_store_id(
        &self,
        cli: &Cli,
        direct_secret: Option<&str>,
        secret_id: Option<&str>,
        secret_id_flag: &str,
    ) -> Result<Option<String>>;
    fn handle_daemon_commands(&self, cli: &Cli) -> Result<bool>;
}

/// Executes startup preflight commands and returns whether startup was handled.
pub fn execute_startup_preflight(cli: &Cli, actions: &dyn StartupPreflightActions) -> Result<bool> {
    if cli.onboard {
        actions.execute_onboarding_command(cli)?;
        return Ok(true);
    }

    if let Some(inspect_file) = cli.multi_channel_route_inspect_file.as_ref() {
        let report = tau_multi_channel::build_multi_channel_route_inspect_report(
            &tau_multi_channel::MultiChannelRouteInspectConfig {
                inspect_file: inspect_file.clone(),
                state_dir: cli.multi_channel_state_dir.clone(),
                orchestrator_route_table_path: cli.orchestrator_route_table.clone(),
            },
        )?;
        if cli.multi_channel_route_inspect_json {
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .context("failed to render multi-channel route inspect json")?
            );
        } else {
            println!(
                "{}",
                tau_multi_channel::render_multi_channel_route_inspect_report(&report)
            );
        }
        return Ok(true);
    }

    if cli.multi_channel_incident_timeline {
        validate_multi_channel_incident_timeline_cli(cli)?;
        let report = tau_multi_channel::build_multi_channel_incident_timeline_report(
            &tau_multi_channel::MultiChannelIncidentTimelineQuery {
                state_dir: cli.multi_channel_state_dir.clone(),
                window_start_unix_ms: cli.multi_channel_incident_start_unix_ms,
                window_end_unix_ms: cli.multi_channel_incident_end_unix_ms,
                event_limit: cli.multi_channel_incident_event_limit.unwrap_or(200),
                replay_export_path: cli.multi_channel_incident_replay_export.clone(),
            },
        )?;
        if cli.multi_channel_incident_timeline_json {
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .context("failed to render multi-channel incident timeline json")?
            );
        } else {
            println!(
                "{}",
                tau_multi_channel::render_multi_channel_incident_timeline_report(&report)
            );
        }
        return Ok(true);
    }

    if cli.multi_channel_send.is_some() {
        validate_multi_channel_send_cli(cli)?;
        actions.execute_multi_channel_send_command(cli)?;
        return Ok(true);
    }

    if cli.multi_channel_channel_status.is_some()
        || cli.multi_channel_channel_login.is_some()
        || cli.multi_channel_channel_logout.is_some()
        || cli.multi_channel_channel_probe.is_some()
    {
        validate_multi_channel_channel_lifecycle_cli(cli)?;
        actions.execute_multi_channel_channel_lifecycle_command(cli)?;
        return Ok(true);
    }

    if cli.deployment_wasm_package_module.is_some() {
        validate_deployment_wasm_package_cli(cli)?;
        actions.execute_deployment_wasm_package_command(cli)?;
        return Ok(true);
    }

    if cli.deployment_wasm_inspect_manifest.is_some() {
        validate_deployment_wasm_inspect_cli(cli)?;
        actions.execute_deployment_wasm_inspect_command(cli)?;
        return Ok(true);
    }

    if cli.session_validate {
        validate_session_file(&cli.session, cli.no_session)?;
        return Ok(true);
    }

    if cli.project_index_build
        || cli.project_index_query.is_some()
        || cli.project_index_inspect
        || cli.project_index_json
    {
        validate_project_index_cli(cli)?;
        actions.execute_project_index_command(cli)?;
        return Ok(true);
    }

    if cli.channel_store_inspect.is_some()
        || cli.channel_store_repair.is_some()
        || cli.transport_health_inspect.is_some()
        || cli.github_status_inspect.is_some()
        || cli.operator_control_summary
        || cli.multi_channel_status_inspect
        || cli.dashboard_status_inspect
        || cli.multi_agent_status_inspect
        || cli.gateway_status_inspect
        || cli.deployment_status_inspect
        || cli.custom_command_status_inspect
        || cli.voice_status_inspect
    {
        actions.execute_channel_store_admin_command(cli)?;
        return Ok(true);
    }

    if cli.gateway_remote_profile_inspect {
        validate_gateway_remote_profile_inspect_cli(cli)?;
        tau_cli::gateway_remote_profile::execute_gateway_remote_profile_inspect_command(cli)?;
        return Ok(true);
    }

    if cli.gateway_remote_plan {
        validate_gateway_remote_plan_cli(cli)?;
        tau_cli::gateway_remote_profile::execute_gateway_remote_plan_command(cli)?;
        return Ok(true);
    }

    if actions.handle_daemon_commands(cli)? {
        return Ok(true);
    }

    if cli.gateway_service_start || cli.gateway_service_stop || cli.gateway_service_status {
        validate_gateway_service_cli(cli)?;
        if cli.gateway_service_start {
            let report = start_gateway_service_mode(&cli.gateway_state_dir)?;
            println!("{}", render_gateway_service_status_report(&report));
            return Ok(true);
        }
        if cli.gateway_service_stop {
            let report = stop_gateway_service_mode(
                &cli.gateway_state_dir,
                cli.gateway_service_stop_reason.as_deref(),
            )?;
            println!("{}", render_gateway_service_status_report(&report));
            return Ok(true);
        }
        if cli.gateway_service_status {
            let report = inspect_gateway_service_mode(&cli.gateway_state_dir)?;
            if cli.gateway_service_status_json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report)
                        .context("failed to render gateway service status json")?
                );
            } else {
                println!("{}", render_gateway_service_status_report(&report));
            }
            return Ok(true);
        }
    }

    if cli.multi_channel_live_ingest_file.is_some() {
        validate_multi_channel_live_ingest_cli(cli)?;
        let payload_file = cli
            .multi_channel_live_ingest_file
            .clone()
            .ok_or_else(|| anyhow!("--multi-channel-live-ingest-file is required"))?;
        let transport: tau_multi_channel::MultiChannelTransport = cli
            .multi_channel_live_ingest_transport
            .ok_or_else(|| anyhow!("--multi-channel-live-ingest-transport is required"))?
            .into();
        let report = tau_multi_channel::ingest_multi_channel_live_raw_payload(
            &tau_multi_channel::MultiChannelLivePayloadIngestConfig {
                ingress_dir: cli.multi_channel_live_ingest_dir.clone(),
                payload_file,
                transport,
                provider: cli.multi_channel_live_ingest_provider.clone(),
            },
        )?;
        println!(
            "multi-channel live ingest queued: transport={} provider={} event_id={} conversation_id={} ingress_file={}",
            report.transport.as_str(),
            report.provider,
            report.event_id,
            report.conversation_id,
            report.ingress_path.display()
        );
        return Ok(true);
    }

    if cli.multi_channel_live_readiness_preflight {
        actions.execute_multi_channel_live_readiness_preflight_command(cli)?;
        return Ok(true);
    }

    if cli.browser_automation_preflight {
        validate_browser_automation_preflight_cli(cli)?;
        actions.execute_browser_automation_preflight_command(cli)?;
        return Ok(true);
    }

    if cli.multi_channel_live_connectors_status {
        let report = tau_multi_channel::load_multi_channel_live_connectors_status_report(
            &cli.multi_channel_live_connectors_state_path,
        )?;
        if cli.multi_channel_live_connectors_status_json {
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .context("failed to render live connector status json")?
            );
        } else {
            let mut channel_lines = Vec::new();
            for (channel, status) in &report.channels {
                let operator_guidance = if status.breaker_state == "open" {
                    format!(
                        "wait_for_breaker_recovery_until:{}",
                        status.breaker_open_until_unix_ms
                    )
                } else if status.liveness == "degraded" {
                    "inspect_provider_errors_and_credentials".to_string()
                } else {
                    "none".to_string()
                };
                channel_lines.push(format!(
                    "{}:mode={} liveness={} breaker_state={} retry_budget_remaining={} breaker_open_until_unix_ms={} breaker_last_open_reason={} ingested={} duplicates={} retries={} auth_failures={} parse_failures={} provider_failures={} last_error_code={} guidance={}",
                    channel,
                    if status.mode.is_empty() { "unknown" } else { status.mode.as_str() },
                    if status.liveness.is_empty() { "unknown" } else { status.liveness.as_str() },
                    if status.breaker_state.is_empty() {
                        "unknown"
                    } else {
                        status.breaker_state.as_str()
                    },
                    status.retry_budget_remaining,
                    status.breaker_open_until_unix_ms,
                    if status.breaker_last_open_reason.is_empty() {
                        "none"
                    } else {
                        status.breaker_last_open_reason.as_str()
                    },
                    status.events_ingested,
                    status.duplicates_skipped,
                    status.retry_attempts,
                    status.auth_failures,
                    status.parse_failures,
                    status.provider_failures,
                    if status.last_error_code.is_empty() {
                        "none"
                    } else {
                        status.last_error_code.as_str()
                    },
                    operator_guidance
                ));
            }
            channel_lines.sort();
            println!(
                "multi-channel live connectors status: state_path={} state_present={} schema_version={} processed_event_count={} channels={}",
                report.state_path,
                report.state_present,
                report.schema_version,
                report.processed_event_count,
                if channel_lines.is_empty() {
                    "none".to_string()
                } else {
                    channel_lines.join(" | ")
                }
            );
        }
        return Ok(true);
    }

    if cli.extension_exec_manifest.is_some() {
        actions.execute_extension_exec_command(cli)?;
        return Ok(true);
    }

    if cli.extension_list {
        actions.execute_extension_list_command(cli)?;
        return Ok(true);
    }

    if cli.extension_show.is_some() {
        actions.execute_extension_show_command(cli)?;
        return Ok(true);
    }

    if cli.extension_validate.is_some() {
        actions.execute_extension_validate_command(cli)?;
        return Ok(true);
    }

    if cli.package_validate.is_some() {
        actions.execute_package_validate_command(cli)?;
        return Ok(true);
    }

    if cli.package_show.is_some() {
        actions.execute_package_show_command(cli)?;
        return Ok(true);
    }

    if cli.package_install.is_some() {
        actions.execute_package_install_command(cli)?;
        return Ok(true);
    }

    if cli.package_update.is_some() {
        actions.execute_package_update_command(cli)?;
        return Ok(true);
    }

    if cli.package_list {
        actions.execute_package_list_command(cli)?;
        return Ok(true);
    }

    if cli.package_remove.is_some() {
        actions.execute_package_remove_command(cli)?;
        return Ok(true);
    }

    if cli.package_rollback.is_some() {
        actions.execute_package_rollback_command(cli)?;
        return Ok(true);
    }

    if cli.package_conflicts {
        actions.execute_package_conflicts_command(cli)?;
        return Ok(true);
    }

    if cli.package_activate {
        actions.execute_package_activate_command(cli)?;
        return Ok(true);
    }

    if cli.qa_loop {
        actions.execute_qa_loop_preflight_command(cli)?;
        return Ok(true);
    }

    if cli.mcp_server {
        actions.execute_mcp_server_command(cli)?;
        return Ok(true);
    }

    if cli.rpc_capabilities {
        actions.execute_rpc_capabilities_command(cli)?;
        return Ok(true);
    }

    if cli.rpc_validate_frame_file.is_some() {
        actions.execute_rpc_validate_frame_command(cli)?;
        return Ok(true);
    }

    if cli.rpc_dispatch_frame_file.is_some() {
        actions.execute_rpc_dispatch_frame_command(cli)?;
        return Ok(true);
    }

    if cli.rpc_dispatch_ndjson_file.is_some() {
        actions.execute_rpc_dispatch_ndjson_command(cli)?;
        return Ok(true);
    }

    if cli.rpc_serve_ndjson {
        actions.execute_rpc_serve_ndjson_command(cli)?;
        return Ok(true);
    }

    if cli.events_inspect {
        actions.execute_events_inspect_command(cli)?;
        return Ok(true);
    }

    if cli.events_validate {
        actions.execute_events_validate_command(cli)?;
        return Ok(true);
    }

    if cli.events_simulate {
        actions.execute_events_simulate_command(cli)?;
        return Ok(true);
    }

    if cli.events_dry_run {
        actions.execute_events_dry_run_command(cli)?;
        return Ok(true);
    }

    if cli.events_template_write.is_some() {
        actions.execute_events_template_write_command(cli)?;
        return Ok(true);
    }

    if cli.event_webhook_ingest_file.is_some() {
        validate_event_webhook_ingest_cli(cli)?;
        let payload_file = cli
            .event_webhook_ingest_file
            .clone()
            .ok_or_else(|| anyhow!("--event-webhook-ingest-file is required"))?;
        let channel_ref = cli
            .event_webhook_channel
            .clone()
            .ok_or_else(|| anyhow!("--event-webhook-channel is required"))?;
        let pairing_policy = pairing_policy_for_state_dir(&cli.channel_store_root);
        let actor_id = cli.event_webhook_actor_id.clone().unwrap_or_default();
        let pairing_decision = evaluate_pairing_access(
            &pairing_policy,
            &channel_ref,
            &actor_id,
            current_unix_timestamp_ms(),
        )?;
        if let PairingDecision::Deny { reason_code } = pairing_decision {
            println!(
                "webhook ingest denied: channel={} actor_id={} reason_code={}",
                channel_ref,
                if actor_id.is_empty() {
                    "(missing)"
                } else {
                    actor_id.as_str()
                },
                reason_code
            );
            return Ok(true);
        }
        let event_webhook_secret = actions.resolve_secret_from_cli_or_store_id(
            cli,
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
            signature_algorithm: map_webhook_signature_algorithm(
                cli.event_webhook_signature_algorithm,
            ),
            signature_max_skew_seconds: cli.event_webhook_signature_max_skew_seconds,
        })?;
        return Ok(true);
    }

    Ok(false)
}

fn map_webhook_signature_algorithm(
    algorithm: Option<tau_cli::CliWebhookSignatureAlgorithm>,
) -> Option<WebhookSignatureAlgorithm> {
    algorithm.map(|value| match value {
        tau_cli::CliWebhookSignatureAlgorithm::GithubSha256 => {
            WebhookSignatureAlgorithm::GithubSha256
        }
        tau_cli::CliWebhookSignatureAlgorithm::SlackV0 => WebhookSignatureAlgorithm::SlackV0,
    })
}
