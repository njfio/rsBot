use super::*;

pub(crate) fn execute_startup_preflight(cli: &Cli) -> Result<bool> {
    if cli.onboard {
        execute_onboarding_command(cli)?;
        return Ok(true);
    }

    if cli.session_validate {
        validate_session_file(cli)?;
        return Ok(true);
    }

    if cli.channel_store_inspect.is_some()
        || cli.channel_store_repair.is_some()
        || cli.transport_health_inspect.is_some()
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

    if cli.gateway_service_start || cli.gateway_service_stop || cli.gateway_service_status {
        validate_gateway_service_cli(cli)?;
        if cli.gateway_service_start {
            let report =
                crate::gateway_runtime::start_gateway_service_mode(&cli.gateway_state_dir)?;
            println!(
                "{}",
                crate::gateway_runtime::render_gateway_service_status_report(&report)
            );
            return Ok(true);
        }
        if cli.gateway_service_stop {
            let report = crate::gateway_runtime::stop_gateway_service_mode(
                &cli.gateway_state_dir,
                cli.gateway_service_stop_reason.as_deref(),
            )?;
            println!(
                "{}",
                crate::gateway_runtime::render_gateway_service_status_report(&report)
            );
            return Ok(true);
        }
        if cli.gateway_service_status {
            let report =
                crate::gateway_runtime::inspect_gateway_service_mode(&cli.gateway_state_dir)?;
            if cli.gateway_service_status_json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report)
                        .context("failed to render gateway service status json")?
                );
            } else {
                println!(
                    "{}",
                    crate::gateway_runtime::render_gateway_service_status_report(&report)
                );
            }
            return Ok(true);
        }
    }

    if cli.multi_channel_live_readiness_preflight {
        execute_multi_channel_live_readiness_preflight_command(cli)?;
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
            signature_algorithm: cli.event_webhook_signature_algorithm.map(Into::into),
            signature_max_skew_seconds: cli.event_webhook_signature_max_skew_seconds,
        })?;
        return Ok(true);
    }

    Ok(false)
}
