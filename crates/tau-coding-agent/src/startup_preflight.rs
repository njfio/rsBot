use super::*;

pub(crate) fn execute_startup_preflight(cli: &Cli) -> Result<bool> {
    if cli.onboard {
        execute_onboarding_command(cli)?;
        return Ok(true);
    }

    if let Some(inspect_file) = cli.multi_channel_route_inspect_file.as_ref() {
        let report = build_multi_channel_route_inspect_report(
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
        let report = build_multi_channel_incident_timeline_report(
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
        crate::multi_channel_send::execute_multi_channel_send_command(cli)?;
        return Ok(true);
    }

    if cli.multi_channel_channel_status.is_some()
        || cli.multi_channel_channel_login.is_some()
        || cli.multi_channel_channel_logout.is_some()
        || cli.multi_channel_channel_probe.is_some()
    {
        validate_multi_channel_channel_lifecycle_cli(cli)?;
        crate::multi_channel_lifecycle::execute_multi_channel_channel_lifecycle_command(cli)?;
        return Ok(true);
    }

    if cli.deployment_wasm_package_module.is_some() {
        validate_deployment_wasm_package_cli(cli)?;
        crate::deployment_wasm::execute_deployment_wasm_package_command(cli)?;
        return Ok(true);
    }

    if cli.deployment_wasm_inspect_manifest.is_some() {
        validate_deployment_wasm_inspect_cli(cli)?;
        crate::deployment_wasm::execute_deployment_wasm_inspect_command(cli)?;
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
        execute_project_index_command(cli)?;
        return Ok(true);
    }

    if crate::daemon_runtime::tau_daemon_mode_requested(cli) {
        validate_daemon_cli(cli)?;
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
        execute_channel_store_admin_command(cli)?;
        return Ok(true);
    }

    if cli.gateway_remote_profile_inspect {
        crate::runtime_cli_validation::validate_gateway_remote_profile_inspect_cli(cli)?;
        crate::gateway_remote_profile::execute_gateway_remote_profile_inspect_command(cli)?;
        return Ok(true);
    }

    if crate::daemon_runtime::tau_daemon_mode_requested(cli) {
        validate_daemon_cli(cli)?;
        let config = crate::daemon_runtime::TauDaemonConfig {
            state_dir: cli.daemon_state_dir.clone(),
            profile: cli.daemon_profile,
        };

        if cli.daemon_install {
            let report = crate::daemon_runtime::install_tau_daemon(&config)?;
            println!(
                "{}",
                crate::daemon_runtime::render_tau_daemon_status_report(&report)
            );
            return Ok(true);
        }
        if cli.daemon_uninstall {
            let report = crate::daemon_runtime::uninstall_tau_daemon(&config)?;
            println!(
                "{}",
                crate::daemon_runtime::render_tau_daemon_status_report(&report)
            );
            return Ok(true);
        }
        if cli.daemon_start {
            let report = crate::daemon_runtime::start_tau_daemon(&config)?;
            println!(
                "{}",
                crate::daemon_runtime::render_tau_daemon_status_report(&report)
            );
            return Ok(true);
        }
        if cli.daemon_stop {
            let report =
                crate::daemon_runtime::stop_tau_daemon(&config, cli.daemon_stop_reason.as_deref())?;
            println!(
                "{}",
                crate::daemon_runtime::render_tau_daemon_status_report(&report)
            );
            return Ok(true);
        }
        if cli.daemon_status {
            let report = crate::daemon_runtime::inspect_tau_daemon(&config)?;
            if cli.daemon_status_json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report)
                        .context("failed to render daemon status json")?
                );
            } else {
                println!(
                    "{}",
                    crate::daemon_runtime::render_tau_daemon_status_report(&report)
                );
            }
            return Ok(true);
        }
    }

    if cli.gateway_service_start || cli.gateway_service_stop || cli.gateway_service_status {
        validate_gateway_service_cli(cli)?;
        if cli.gateway_service_start {
            let report = tau_gateway::start_gateway_service_mode(&cli.gateway_state_dir)?;
            println!(
                "{}",
                tau_gateway::render_gateway_service_status_report(&report)
            );
            return Ok(true);
        }
        if cli.gateway_service_stop {
            let report = tau_gateway::stop_gateway_service_mode(
                &cli.gateway_state_dir,
                cli.gateway_service_stop_reason.as_deref(),
            )?;
            println!(
                "{}",
                tau_gateway::render_gateway_service_status_report(&report)
            );
            return Ok(true);
        }
        if cli.gateway_service_status {
            let report = tau_gateway::inspect_gateway_service_mode(&cli.gateway_state_dir)?;
            if cli.gateway_service_status_json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report)
                        .context("failed to render gateway service status json")?
                );
            } else {
                println!(
                    "{}",
                    tau_gateway::render_gateway_service_status_report(&report)
                );
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
        let transport: crate::multi_channel_contract::MultiChannelTransport = cli
            .multi_channel_live_ingest_transport
            .ok_or_else(|| anyhow!("--multi-channel-live-ingest-transport is required"))?
            .into();
        let report = crate::multi_channel_live_ingress::ingest_multi_channel_live_raw_payload(
            &crate::multi_channel_live_ingress::MultiChannelLivePayloadIngestConfig {
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
        execute_multi_channel_live_readiness_preflight_command(cli)?;
        return Ok(true);
    }

    if cli.browser_automation_preflight {
        validate_browser_automation_preflight_cli(cli)?;
        execute_browser_automation_preflight_command(cli)?;
        return Ok(true);
    }

    if cli.multi_channel_live_connectors_status {
        let report =
            crate::multi_channel_live_connectors::load_multi_channel_live_connectors_status_report(
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
        execute_extension_exec_command(cli)?;
        return Ok(true);
    }

    if cli.extension_list {
        execute_extension_list_command(cli)?;
        return Ok(true);
    }

    if cli.extension_show.is_some() {
        execute_extension_show_command(cli)?;
        return Ok(true);
    }

    if cli.extension_validate.is_some() {
        execute_extension_validate_command(cli)?;
        return Ok(true);
    }

    if cli.package_validate.is_some() {
        execute_package_validate_command(cli)?;
        return Ok(true);
    }

    if cli.package_show.is_some() {
        execute_package_show_command(cli)?;
        return Ok(true);
    }

    if cli.package_install.is_some() {
        execute_package_install_command(cli)?;
        return Ok(true);
    }

    if cli.package_update.is_some() {
        execute_package_update_command(cli)?;
        return Ok(true);
    }

    if cli.package_list {
        execute_package_list_command(cli)?;
        return Ok(true);
    }

    if cli.package_remove.is_some() {
        execute_package_remove_command(cli)?;
        return Ok(true);
    }

    if cli.package_rollback.is_some() {
        execute_package_rollback_command(cli)?;
        return Ok(true);
    }

    if cli.package_conflicts {
        execute_package_conflicts_command(cli)?;
        return Ok(true);
    }

    if cli.package_activate {
        execute_package_activate_command(cli)?;
        return Ok(true);
    }

    if cli.qa_loop {
        execute_qa_loop_preflight_command(cli)?;
        return Ok(true);
    }

    if cli.mcp_server {
        execute_mcp_server_command(cli)?;
        return Ok(true);
    }

    if cli.rpc_capabilities {
        execute_rpc_capabilities_command(cli)?;
        return Ok(true);
    }

    if cli.rpc_validate_frame_file.is_some() {
        execute_rpc_validate_frame_command(cli)?;
        return Ok(true);
    }

    if cli.rpc_dispatch_frame_file.is_some() {
        execute_rpc_dispatch_frame_command(cli)?;
        return Ok(true);
    }

    if cli.rpc_dispatch_ndjson_file.is_some() {
        execute_rpc_dispatch_ndjson_command(cli)?;
        return Ok(true);
    }

    if cli.rpc_serve_ndjson {
        execute_rpc_serve_ndjson_command(cli)?;
        return Ok(true);
    }

    if cli.events_inspect {
        execute_events_inspect_command(cli)?;
        return Ok(true);
    }

    if cli.events_validate {
        execute_events_validate_command(cli)?;
        return Ok(true);
    }

    if cli.events_simulate {
        execute_events_simulate_command(cli)?;
        return Ok(true);
    }

    if cli.events_dry_run {
        execute_events_dry_run_command(cli)?;
        return Ok(true);
    }

    if cli.events_template_write.is_some() {
        execute_events_template_write_command(cli)?;
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
        let event_webhook_secret = resolve_secret_from_cli_or_store_id(
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
            signature_algorithm: cli
                .event_webhook_signature_algorithm
                .map(crate::cli_types::map_webhook_signature_algorithm),
            signature_max_skew_seconds: cli.event_webhook_signature_max_skew_seconds,
        })?;
        return Ok(true);
    }

    Ok(false)
}
