//! Gateway status endpoint runtime helpers.

use super::*;

pub(super) async fn handle_gateway_status(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
) -> Response {
    let principal = match authorize_gateway_request(&state, &headers) {
        Ok(principal) => principal,
        Err(error) => return error.into_response(),
    };
    if let Err(error) = enforce_gateway_rate_limit(&state, principal.as_str()) {
        return error.into_response();
    }

    let service_report =
        match crate::gateway_runtime::inspect_gateway_service_mode(&state.config.state_dir) {
            Ok(report) => report,
            Err(error) => {
                return OpenResponsesApiError::internal(format!(
                    "failed to inspect gateway service state: {error}"
                ))
                .into_response();
            }
        };
    let multi_channel_report = collect_gateway_multi_channel_status_report(&state.config.state_dir);
    let events_report = collect_gateway_events_status_report(&state.config.state_dir);
    let dashboard_snapshot = collect_gateway_dashboard_snapshot(&state.config.state_dir);
    let runtime_heartbeat = inspect_runtime_heartbeat(&state.config.runtime_heartbeat.state_path);

    (
        StatusCode::OK,
        Json(json!({
            "service": service_report,
            "auth": collect_gateway_auth_status_report(&state),
            "multi_channel": multi_channel_report,
            "events": events_report,
            "training": dashboard_snapshot.training,
            "runtime_heartbeat": runtime_heartbeat,
            "gateway": {
                "responses_endpoint": OPENRESPONSES_ENDPOINT,
                "openai_compat": {
                    "chat_completions_endpoint": OPENAI_CHAT_COMPLETIONS_ENDPOINT,
                    "completions_endpoint": OPENAI_COMPLETIONS_ENDPOINT,
                    "models_endpoint": OPENAI_MODELS_ENDPOINT,
                    "runtime": state.collect_openai_compat_status_report(),
                },
                "web_ui": {
                    "sessions_endpoint": GATEWAY_SESSIONS_ENDPOINT,
                    "session_detail_endpoint": GATEWAY_SESSION_DETAIL_ENDPOINT,
                    "session_append_endpoint": GATEWAY_SESSION_APPEND_ENDPOINT,
                    "session_reset_endpoint": GATEWAY_SESSION_RESET_ENDPOINT,
                    "memory_endpoint": GATEWAY_MEMORY_ENDPOINT,
                    "memory_entry_endpoint": GATEWAY_MEMORY_ENTRY_ENDPOINT,
                    "memory_graph_endpoint": GATEWAY_MEMORY_GRAPH_ENDPOINT,
                    "memory_graph_api_endpoint": API_MEMORIES_GRAPH_ENDPOINT,
                    "channel_lifecycle_endpoint": GATEWAY_CHANNEL_LIFECYCLE_ENDPOINT,
                    "config_endpoint": GATEWAY_CONFIG_ENDPOINT,
                    "safety_policy_endpoint": GATEWAY_SAFETY_POLICY_ENDPOINT,
                    "safety_rules_endpoint": GATEWAY_SAFETY_RULES_ENDPOINT,
                    "safety_test_endpoint": GATEWAY_SAFETY_TEST_ENDPOINT,
                    "audit_summary_endpoint": GATEWAY_AUDIT_SUMMARY_ENDPOINT,
                    "audit_log_endpoint": GATEWAY_AUDIT_LOG_ENDPOINT,
                    "training_status_endpoint": GATEWAY_TRAINING_STATUS_ENDPOINT,
                    "training_rollouts_endpoint": GATEWAY_TRAINING_ROLLOUTS_ENDPOINT,
                    "training_config_endpoint": GATEWAY_TRAINING_CONFIG_ENDPOINT,
                    "tools_endpoint": GATEWAY_TOOLS_ENDPOINT,
                    "tool_stats_endpoint": GATEWAY_TOOLS_STATS_ENDPOINT,
                    "jobs_endpoint": GATEWAY_JOBS_ENDPOINT,
                    "job_cancel_endpoint_template": GATEWAY_JOB_CANCEL_ENDPOINT_TEMPLATE,
                    "deploy_endpoint": GATEWAY_DEPLOY_ENDPOINT,
                    "agent_stop_endpoint_template": GATEWAY_AGENT_STOP_ENDPOINT_TEMPLATE,
                    "ui_telemetry_endpoint": GATEWAY_UI_TELEMETRY_ENDPOINT,
                    "cortex_chat_endpoint": CORTEX_CHAT_ENDPOINT,
                    "cortex_status_endpoint": CORTEX_STATUS_ENDPOINT,
                    "policy_gates": {
                        "session_write": SESSION_WRITE_POLICY_GATE,
                        "memory_write": MEMORY_WRITE_POLICY_GATE,
                    },
                    "telemetry_runtime": state.collect_ui_telemetry_status_report(),
                },
                "dashboard_shell_endpoint": DASHBOARD_SHELL_ENDPOINT,
                "ops_dashboard_endpoint": OPS_DASHBOARD_ENDPOINT,
                "ops_dashboard_login_endpoint": OPS_DASHBOARD_LOGIN_ENDPOINT,
                "webchat_endpoint": WEBCHAT_ENDPOINT,
                "auth_bootstrap_endpoint": GATEWAY_AUTH_BOOTSTRAP_ENDPOINT,
                "auth_session_endpoint": GATEWAY_AUTH_SESSION_ENDPOINT,
                "status_endpoint": GATEWAY_STATUS_ENDPOINT,
                "ws_endpoint": GATEWAY_WS_ENDPOINT,
                "dashboard": {
                    "health_endpoint": DASHBOARD_HEALTH_ENDPOINT,
                    "widgets_endpoint": DASHBOARD_WIDGETS_ENDPOINT,
                    "queue_timeline_endpoint": DASHBOARD_QUEUE_TIMELINE_ENDPOINT,
                    "alerts_endpoint": DASHBOARD_ALERTS_ENDPOINT,
                    "actions_endpoint": DASHBOARD_ACTIONS_ENDPOINT,
                    "stream_endpoint": DASHBOARD_STREAM_ENDPOINT,
                },
                "external_coding_agent": {
                    "sessions_endpoint": EXTERNAL_CODING_AGENT_SESSIONS_ENDPOINT,
                    "session_detail_endpoint": EXTERNAL_CODING_AGENT_SESSION_DETAIL_ENDPOINT,
                    "progress_endpoint": EXTERNAL_CODING_AGENT_SESSION_PROGRESS_ENDPOINT,
                    "followups_endpoint": EXTERNAL_CODING_AGENT_SESSION_FOLLOWUPS_ENDPOINT,
                    "followups_drain_endpoint": EXTERNAL_CODING_AGENT_SESSION_FOLLOWUPS_DRAIN_ENDPOINT,
                    "stream_endpoint": EXTERNAL_CODING_AGENT_SESSION_STREAM_ENDPOINT,
                    "close_endpoint": EXTERNAL_CODING_AGENT_SESSION_CLOSE_ENDPOINT,
                    "reap_endpoint": EXTERNAL_CODING_AGENT_REAP_ENDPOINT,
                    "runtime": {
                        "active_sessions": state.external_coding_agent_bridge.active_session_count(),
                        "inactivity_timeout_ms": state.config.external_coding_agent_bridge.inactivity_timeout_ms,
                        "max_active_sessions": state.config.external_coding_agent_bridge.max_active_sessions,
                        "max_events_per_session": state.config.external_coding_agent_bridge.max_events_per_session,
                    }
                },
                "state_dir": state.config.state_dir.display().to_string(),
                "model": state.config.model,
            }
        })),
    )
        .into_response()
}
