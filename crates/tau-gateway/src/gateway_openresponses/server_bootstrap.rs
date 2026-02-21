//! Gateway OpenResponses server bootstrap and router wiring.

use super::*;

/// Public `fn` `run_gateway_openresponses_server` in `tau-gateway`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
pub async fn run_gateway_openresponses_server(
    config: GatewayOpenResponsesServerConfig,
) -> Result<()> {
    std::fs::create_dir_all(&config.state_dir)
        .with_context(|| format!("failed to create {}", config.state_dir.display()))?;

    let bind_addr = config
        .bind
        .parse::<SocketAddr>()
        .with_context(|| format!("invalid --gateway-openresponses-bind '{}'", config.bind))?;

    let service_report = crate::gateway_runtime::start_gateway_service_mode(&config.state_dir)?;
    println!(
        "{}",
        crate::gateway_runtime::render_gateway_service_status_report(&service_report)
    );

    let listener = TcpListener::bind(bind_addr)
        .await
        .with_context(|| format!("failed to bind gateway openresponses server on {bind_addr}"))?;
    let local_addr = listener
        .local_addr()
        .context("failed to resolve bound openresponses server address")?;
    let mut runtime_heartbeat_handle =
        start_runtime_heartbeat_scheduler(config.runtime_heartbeat.clone())?;

    println!(
        "gateway openresponses server listening: endpoint={} addr={} state_dir={}",
        OPENRESPONSES_ENDPOINT,
        local_addr,
        config.state_dir.display()
    );

    let state_dir = config.state_dir.clone();
    let state = Arc::new(GatewayOpenResponsesServerState::new(config));
    let mut cortex_bulletin_runtime = start_cortex_bulletin_runtime(
        Arc::clone(&state.cortex),
        state.config.client.clone(),
        state.config.model.clone(),
        state.config.runtime_heartbeat.enabled,
        state.config.runtime_heartbeat.interval,
    );
    let app = build_gateway_openresponses_router(state);
    let serve_result = axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await;
    cortex_bulletin_runtime.shutdown().await;
    runtime_heartbeat_handle.shutdown().await;
    serve_result.context("gateway openresponses server exited unexpectedly")?;

    let stop_report = crate::gateway_runtime::stop_gateway_service_mode(
        &state_dir,
        Some("openresponses_server_shutdown"),
    );
    if let Ok(report) = stop_report {
        println!(
            "{}",
            crate::gateway_runtime::render_gateway_service_status_report(&report)
        );
    }

    Ok(())
}

pub(super) fn build_gateway_openresponses_router(
    state: Arc<GatewayOpenResponsesServerState>,
) -> Router {
    Router::new()
        .route(OPENRESPONSES_ENDPOINT, post(handle_openresponses))
        .route(
            OPENAI_CHAT_COMPLETIONS_ENDPOINT,
            post(handle_openai_chat_completions),
        )
        .route(OPENAI_COMPLETIONS_ENDPOINT, post(handle_openai_completions))
        .route(OPENAI_MODELS_ENDPOINT, get(handle_openai_models))
        .route(
            GATEWAY_AUTH_SESSION_ENDPOINT,
            post(handle_gateway_auth_session),
        )
        .route(GATEWAY_SESSIONS_ENDPOINT, get(handle_gateway_sessions_list))
        .route(
            GATEWAY_SESSION_DETAIL_ENDPOINT,
            get(handle_gateway_session_detail),
        )
        .route(
            GATEWAY_SESSION_APPEND_ENDPOINT,
            post(handle_gateway_session_append),
        )
        .route(
            GATEWAY_SESSION_RESET_ENDPOINT,
            post(handle_gateway_session_reset),
        )
        .route(
            GATEWAY_MEMORY_ENDPOINT,
            get(handle_gateway_memory_read).put(handle_gateway_memory_write),
        )
        .route(
            GATEWAY_MEMORY_ENTRY_ENDPOINT,
            get(handle_gateway_memory_entry_read)
                .put(handle_gateway_memory_entry_write)
                .delete(handle_gateway_memory_entry_delete),
        )
        .route(
            GATEWAY_MEMORY_GRAPH_ENDPOINT,
            get(handle_gateway_memory_graph),
        )
        .route(API_MEMORIES_GRAPH_ENDPOINT, get(handle_api_memories_graph))
        .route(
            GATEWAY_CHANNEL_LIFECYCLE_ENDPOINT,
            post(handle_gateway_channel_lifecycle_action),
        )
        .route(
            GATEWAY_CONFIG_ENDPOINT,
            get(handle_gateway_config_get).patch(handle_gateway_config_patch),
        )
        .route(
            GATEWAY_SAFETY_POLICY_ENDPOINT,
            get(handle_gateway_safety_policy_get).put(handle_gateway_safety_policy_put),
        )
        .route(
            GATEWAY_SAFETY_RULES_ENDPOINT,
            get(handle_gateway_safety_rules_get).put(handle_gateway_safety_rules_put),
        )
        .route(
            GATEWAY_SAFETY_TEST_ENDPOINT,
            post(handle_gateway_safety_test),
        )
        .route(
            GATEWAY_AUDIT_SUMMARY_ENDPOINT,
            get(handle_gateway_audit_summary),
        )
        .route(GATEWAY_AUDIT_LOG_ENDPOINT, get(handle_gateway_audit_log))
        .route(
            GATEWAY_TRAINING_STATUS_ENDPOINT,
            get(handle_gateway_training_status),
        )
        .route(
            GATEWAY_TRAINING_ROLLOUTS_ENDPOINT,
            get(handle_gateway_training_rollouts),
        )
        .route(
            GATEWAY_TRAINING_CONFIG_ENDPOINT,
            patch(handle_gateway_training_config_patch),
        )
        .route(GATEWAY_TOOLS_ENDPOINT, get(handle_gateway_tools_inventory))
        .route(
            GATEWAY_TOOLS_STATS_ENDPOINT,
            get(handle_gateway_tools_stats),
        )
        .route(GATEWAY_JOBS_ENDPOINT, get(handle_gateway_jobs_list))
        .route(
            GATEWAY_JOB_CANCEL_ENDPOINT_TEMPLATE,
            post(handle_gateway_job_cancel),
        )
        .route(GATEWAY_DEPLOY_ENDPOINT, post(handle_gateway_deploy))
        .route(
            GATEWAY_AGENT_STOP_ENDPOINT_TEMPLATE,
            post(handle_gateway_agent_stop),
        )
        .route(
            GATEWAY_UI_TELEMETRY_ENDPOINT,
            post(handle_gateway_ui_telemetry),
        )
        .route(CORTEX_CHAT_ENDPOINT, post(handle_cortex_chat))
        .route(CORTEX_STATUS_ENDPOINT, get(handle_cortex_status))
        .route(
            EXTERNAL_CODING_AGENT_SESSIONS_ENDPOINT,
            post(handle_external_coding_agent_open_session),
        )
        .route(
            EXTERNAL_CODING_AGENT_SESSION_DETAIL_ENDPOINT,
            get(handle_external_coding_agent_session_detail),
        )
        .route(
            EXTERNAL_CODING_AGENT_SESSION_PROGRESS_ENDPOINT,
            post(handle_external_coding_agent_session_progress),
        )
        .route(
            EXTERNAL_CODING_AGENT_SESSION_FOLLOWUPS_ENDPOINT,
            post(handle_external_coding_agent_session_followup),
        )
        .route(
            EXTERNAL_CODING_AGENT_SESSION_FOLLOWUPS_DRAIN_ENDPOINT,
            post(handle_external_coding_agent_session_followups_drain),
        )
        .route(
            EXTERNAL_CODING_AGENT_SESSION_STREAM_ENDPOINT,
            get(handle_external_coding_agent_session_stream),
        )
        .route(
            EXTERNAL_CODING_AGENT_SESSION_CLOSE_ENDPOINT,
            post(handle_external_coding_agent_session_close),
        )
        .route(
            EXTERNAL_CODING_AGENT_REAP_ENDPOINT,
            post(handle_external_coding_agent_reap),
        )
        .route(OPS_DASHBOARD_ENDPOINT, get(handle_ops_dashboard_shell_page))
        .route(
            OPS_DASHBOARD_AGENTS_ENDPOINT,
            get(handle_ops_dashboard_agents_shell_page),
        )
        .route(
            OPS_DASHBOARD_AGENT_DETAIL_ENDPOINT,
            get(handle_ops_dashboard_agent_detail_shell_page),
        )
        .route(
            OPS_DASHBOARD_CHAT_ENDPOINT,
            get(handle_ops_dashboard_chat_shell_page),
        )
        .route(
            OPS_DASHBOARD_CHAT_NEW_ENDPOINT,
            post(handle_ops_dashboard_chat_new),
        )
        .route(
            OPS_DASHBOARD_CHAT_SEND_ENDPOINT,
            post(handle_ops_dashboard_chat_send),
        )
        .route(
            OPS_DASHBOARD_SESSIONS_ENDPOINT,
            get(handle_ops_dashboard_sessions_shell_page),
        )
        .route(
            "/ops/sessions/branch",
            post(handle_ops_dashboard_sessions_branch),
        )
        .route(
            OPS_DASHBOARD_SESSION_DETAIL_ENDPOINT,
            get(handle_ops_dashboard_session_detail_shell_page)
                .post(handle_ops_dashboard_session_detail_reset),
        )
        .route(
            OPS_DASHBOARD_MEMORY_ENDPOINT,
            get(handle_ops_dashboard_memory_shell_page).post(handle_ops_dashboard_memory_create),
        )
        .route(
            OPS_DASHBOARD_MEMORY_GRAPH_ENDPOINT,
            get(handle_ops_dashboard_memory_graph_shell_page),
        )
        .route(
            OPS_DASHBOARD_TOOLS_JOBS_ENDPOINT,
            get(handle_ops_dashboard_tools_jobs_shell_page),
        )
        .route(
            OPS_DASHBOARD_CHANNELS_ENDPOINT,
            get(handle_ops_dashboard_channels_shell_page),
        )
        .route(
            OPS_DASHBOARD_CONFIG_ENDPOINT,
            get(handle_ops_dashboard_config_shell_page),
        )
        .route(
            OPS_DASHBOARD_TRAINING_ENDPOINT,
            get(handle_ops_dashboard_training_shell_page),
        )
        .route(
            OPS_DASHBOARD_SAFETY_ENDPOINT,
            get(handle_ops_dashboard_safety_shell_page),
        )
        .route(
            OPS_DASHBOARD_DIAGNOSTICS_ENDPOINT,
            get(handle_ops_dashboard_diagnostics_shell_page),
        )
        .route(
            OPS_DASHBOARD_DEPLOY_ENDPOINT,
            get(handle_ops_dashboard_deploy_shell_page),
        )
        .route(
            OPS_DASHBOARD_LOGIN_ENDPOINT,
            get(handle_ops_dashboard_login_shell_page),
        )
        .route(DASHBOARD_SHELL_ENDPOINT, get(handle_dashboard_shell_page))
        .route(WEBCHAT_ENDPOINT, get(handle_webchat_page))
        .route(
            GATEWAY_AUTH_BOOTSTRAP_ENDPOINT,
            get(handle_gateway_auth_bootstrap),
        )
        .route(GATEWAY_STATUS_ENDPOINT, get(handle_gateway_status))
        .route(DASHBOARD_HEALTH_ENDPOINT, get(handle_dashboard_health))
        .route(DASHBOARD_WIDGETS_ENDPOINT, get(handle_dashboard_widgets))
        .route(
            DASHBOARD_QUEUE_TIMELINE_ENDPOINT,
            get(handle_dashboard_queue_timeline),
        )
        .route(DASHBOARD_ALERTS_ENDPOINT, get(handle_dashboard_alerts))
        .route(DASHBOARD_ACTIONS_ENDPOINT, post(handle_dashboard_action))
        .route(DASHBOARD_STREAM_ENDPOINT, get(handle_dashboard_stream))
        .route(GATEWAY_WS_ENDPOINT, get(handle_gateway_ws_upgrade))
        .with_state(state)
}
