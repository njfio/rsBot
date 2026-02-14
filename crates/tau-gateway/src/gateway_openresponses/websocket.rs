//! WebSocket control-plane handling for the OpenResponses gateway.
use super::auth_runtime::prune_expired_gateway_sessions;
use super::*;

fn gateway_ws_message_from_frame(
    frame: &crate::gateway_ws_protocol::GatewayWsResponseFrame,
) -> WsMessage {
    match serde_json::to_string(frame) {
        Ok(raw) => WsMessage::Text(raw.into()),
        Err(error) => {
            let fallback = crate::gateway_ws_protocol::build_gateway_ws_error_frame(
                &frame.request_id,
                crate::gateway_ws_protocol::GATEWAY_WS_ERROR_CODE_INTERNAL_ERROR,
                format!("failed to serialize gateway websocket frame: {error}").as_str(),
            );
            WsMessage::Text(
                serde_json::to_string(&fallback)
                    .unwrap_or_else(|_| {
                        "{\"schema_version\":1,\"request_id\":\"unknown-request\",\"kind\":\"error\",\"payload\":{\"code\":\"internal_error\",\"message\":\"failed to serialize gateway websocket frame\"}}".to_string()
                    })
                    .into(),
            )
        }
    }
}

fn gateway_ws_error_code_from_api_error(error: &OpenResponsesApiError) -> &'static str {
    match error.code {
        "unauthorized" => crate::gateway_ws_protocol::GATEWAY_WS_ERROR_CODE_UNAUTHORIZED,
        "rate_limited" => crate::gateway_ws_protocol::GATEWAY_WS_ERROR_CODE_RATE_LIMITED,
        "internal_error" => crate::gateway_ws_protocol::GATEWAY_WS_ERROR_CODE_INTERNAL_ERROR,
        other => other,
    }
}

fn enforce_gateway_ws_principal_active(
    state: &GatewayOpenResponsesServerState,
    principal: &str,
) -> Result<(), OpenResponsesApiError> {
    let Some(session_token) = principal.strip_prefix("session:") else {
        return Ok(());
    };

    let now_unix_ms = current_unix_timestamp_ms();
    let mut auth_state = state
        .auth_runtime
        .lock()
        .map_err(|_| OpenResponsesApiError::internal("gateway auth state lock poisoned"))?;
    prune_expired_gateway_sessions(&mut auth_state, now_unix_ms);
    if let Some(session) = auth_state.sessions.get_mut(session_token) {
        session.last_seen_unix_ms = now_unix_ms;
        session.request_count = session.request_count.saturating_add(1);
        return Ok(());
    }
    auth_state.auth_failures = auth_state.auth_failures.saturating_add(1);
    Err(OpenResponsesApiError::unauthorized())
}

fn gateway_ws_resolve_session_key(payload: &serde_json::Map<String, Value>) -> Result<String> {
    let requested = crate::gateway_ws_protocol::parse_optional_session_key(payload)?;
    Ok(sanitize_session_key(
        requested.as_deref().unwrap_or(DEFAULT_SESSION_KEY),
    ))
}

fn collect_gateway_ws_session_status_payload(
    state: &GatewayOpenResponsesServerState,
    payload: &serde_json::Map<String, Value>,
) -> Result<Value> {
    let session_key = gateway_ws_resolve_session_key(payload)?;
    let session_path = gateway_session_path(&state.config.state_dir, &session_key);
    let exists = session_path.exists();
    let (message_count, bytes) = if exists {
        let raw = std::fs::read_to_string(&session_path)
            .with_context(|| format!("failed to read {}", session_path.display()))?;
        (
            raw.lines().filter(|line| !line.trim().is_empty()).count(),
            raw.len(),
        )
    } else {
        (0, 0)
    };
    Ok(json!({
        "session_key": session_key,
        "exists": exists,
        "message_count": message_count,
        "bytes": bytes,
    }))
}

fn collect_gateway_ws_session_reset_payload(
    state: &GatewayOpenResponsesServerState,
    payload: &serde_json::Map<String, Value>,
) -> Result<Value> {
    let session_key = gateway_ws_resolve_session_key(payload)?;
    let session_path = gateway_session_path(&state.config.state_dir, &session_key);
    let lock_path = session_path.with_extension("lock");
    let mut reset = false;

    if session_path.exists() {
        std::fs::remove_file(&session_path)
            .with_context(|| format!("failed to remove {}", session_path.display()))?;
        reset = true;
    }
    if lock_path.exists() {
        std::fs::remove_file(&lock_path)
            .with_context(|| format!("failed to remove {}", lock_path.display()))?;
    }

    Ok(json!({
        "session_key": session_key,
        "reset": reset,
    }))
}

fn collect_gateway_ws_run_lifecycle_status_payload() -> Value {
    let capabilities = crate::gateway_ws_protocol::gateway_ws_capabilities_payload();
    json!({
        "active_runs": [],
        "recent_events": [],
        "event_kinds": capabilities["contracts"]["run_lifecycle"]["event_kinds"].clone(),
    })
}

fn dispatch_gateway_ws_control_text_frame(
    state: &GatewayOpenResponsesServerState,
    principal: &str,
    raw: &str,
) -> (crate::gateway_ws_protocol::GatewayWsResponseFrame, bool) {
    if let Err(error) = enforce_gateway_ws_principal_active(state, principal) {
        let request_id = crate::gateway_ws_protocol::best_effort_gateway_ws_request_id(raw)
            .unwrap_or_else(|| "unknown-request".to_string());
        let code = gateway_ws_error_code_from_api_error(&error);
        return (
            crate::gateway_ws_protocol::build_gateway_ws_error_frame(
                &request_id,
                code,
                error.message.as_str(),
            ),
            true,
        );
    }
    if let Err(error) = enforce_gateway_rate_limit(state, principal) {
        let request_id = crate::gateway_ws_protocol::best_effort_gateway_ws_request_id(raw)
            .unwrap_or_else(|| "unknown-request".to_string());
        let code = gateway_ws_error_code_from_api_error(&error);
        return (
            crate::gateway_ws_protocol::build_gateway_ws_error_frame(
                &request_id,
                code,
                error.message.as_str(),
            ),
            false,
        );
    }

    let request_frame = match crate::gateway_ws_protocol::parse_gateway_ws_request_frame(raw) {
        Ok(frame) => frame,
        Err(error) => {
            let request_id = crate::gateway_ws_protocol::best_effort_gateway_ws_request_id(raw)
                .unwrap_or_else(|| "unknown-request".to_string());
            let message = error.to_string();
            let code =
                crate::gateway_ws_protocol::classify_gateway_ws_parse_error(message.as_str());
            return (
                crate::gateway_ws_protocol::build_gateway_ws_error_frame(
                    &request_id,
                    code,
                    message.as_str(),
                ),
                false,
            );
        }
    };

    let response = match request_frame.kind {
        crate::gateway_ws_protocol::GatewayWsRequestKind::Capabilities => {
            crate::gateway_ws_protocol::build_gateway_ws_response_frame(
                &request_frame.request_id,
                "capabilities.response",
                crate::gateway_ws_protocol::gateway_ws_capabilities_payload(),
            )
        }
        crate::gateway_ws_protocol::GatewayWsRequestKind::GatewayStatus => {
            match crate::gateway_runtime::inspect_gateway_service_mode(&state.config.state_dir) {
                Ok(service_report) => {
                    let multi_channel_report =
                        collect_gateway_multi_channel_status_report(&state.config.state_dir);
                    let dashboard_snapshot =
                        collect_gateway_dashboard_snapshot(&state.config.state_dir);
                    crate::gateway_ws_protocol::build_gateway_ws_response_frame(
                        &request_frame.request_id,
                        "gateway.status.response",
                        json!({
                            "service": service_report,
                            "auth": collect_gateway_auth_status_report(state),
                            "multi_channel": multi_channel_report,
                            "training": dashboard_snapshot.training,
                            "gateway": {
                                "responses_endpoint": OPENRESPONSES_ENDPOINT,
                                "status_endpoint": GATEWAY_STATUS_ENDPOINT,
                                "webchat_endpoint": WEBCHAT_ENDPOINT,
                                "auth_session_endpoint": GATEWAY_AUTH_SESSION_ENDPOINT,
                                "ws_endpoint": GATEWAY_WS_ENDPOINT,
                                "dashboard": {
                                    "health_endpoint": DASHBOARD_HEALTH_ENDPOINT,
                                    "widgets_endpoint": DASHBOARD_WIDGETS_ENDPOINT,
                                    "queue_timeline_endpoint": DASHBOARD_QUEUE_TIMELINE_ENDPOINT,
                                    "alerts_endpoint": DASHBOARD_ALERTS_ENDPOINT,
                                    "actions_endpoint": DASHBOARD_ACTIONS_ENDPOINT,
                                    "stream_endpoint": DASHBOARD_STREAM_ENDPOINT
                                },
                                "model": state.config.model,
                                "state_dir": state.config.state_dir.display().to_string(),
                            }
                        }),
                    )
                }
                Err(error) => crate::gateway_ws_protocol::build_gateway_ws_error_frame(
                    &request_frame.request_id,
                    crate::gateway_ws_protocol::GATEWAY_WS_ERROR_CODE_INTERNAL_ERROR,
                    format!("failed to inspect gateway service state: {error}").as_str(),
                ),
            }
        }
        crate::gateway_ws_protocol::GatewayWsRequestKind::SessionStatus => {
            match collect_gateway_ws_session_status_payload(state, &request_frame.payload) {
                Ok(payload) => crate::gateway_ws_protocol::build_gateway_ws_response_frame(
                    &request_frame.request_id,
                    "session.status.response",
                    payload,
                ),
                Err(error) => {
                    let message = error.to_string();
                    let code = crate::gateway_ws_protocol::classify_gateway_ws_parse_error(
                        message.as_str(),
                    );
                    crate::gateway_ws_protocol::build_gateway_ws_error_frame(
                        &request_frame.request_id,
                        code,
                        message.as_str(),
                    )
                }
            }
        }
        crate::gateway_ws_protocol::GatewayWsRequestKind::SessionReset => {
            match collect_gateway_ws_session_reset_payload(state, &request_frame.payload) {
                Ok(payload) => crate::gateway_ws_protocol::build_gateway_ws_response_frame(
                    &request_frame.request_id,
                    "session.reset.response",
                    payload,
                ),
                Err(error) => {
                    let message = error.to_string();
                    let code = crate::gateway_ws_protocol::classify_gateway_ws_parse_error(
                        message.as_str(),
                    );
                    crate::gateway_ws_protocol::build_gateway_ws_error_frame(
                        &request_frame.request_id,
                        code,
                        message.as_str(),
                    )
                }
            }
        }
        crate::gateway_ws_protocol::GatewayWsRequestKind::RunLifecycleStatus => {
            crate::gateway_ws_protocol::build_gateway_ws_response_frame(
                &request_frame.request_id,
                "run.lifecycle.status.response",
                collect_gateway_ws_run_lifecycle_status_payload(),
            )
        }
    };

    (response, false)
}

pub(super) async fn run_gateway_ws_connection(
    state: Arc<GatewayOpenResponsesServerState>,
    socket: WebSocket,
    principal: String,
) {
    let (mut sender, mut receiver) = socket.split();
    let mut heartbeat = tokio::time::interval(Duration::from_secs(
        crate::gateway_ws_protocol::GATEWAY_WS_HEARTBEAT_INTERVAL_SECONDS.max(1),
    ));
    heartbeat.tick().await;

    loop {
        tokio::select! {
            inbound = receiver.next() => {
                let Some(inbound) = inbound else {
                    break;
                };
                let message = match inbound {
                    Ok(message) => message,
                    Err(_) => break,
                };

                match message {
                    WsMessage::Text(text) => {
                        let (response, should_close) =
                            dispatch_gateway_ws_control_text_frame(&state, principal.as_str(), text.as_str());
                        if sender
                            .send(gateway_ws_message_from_frame(&response))
                            .await
                            .is_err()
                        {
                            break;
                        }
                        if should_close {
                            break;
                        }
                    }
                    WsMessage::Binary(bytes) => {
                        let request_id = "unknown-request";
                        let response = match String::from_utf8(bytes.to_vec()) {
                            Ok(text) => dispatch_gateway_ws_control_text_frame(
                                &state,
                                principal.as_str(),
                                text.as_str(),
                            )
                            .0,
                            Err(_) => crate::gateway_ws_protocol::build_gateway_ws_error_frame(
                                request_id,
                                crate::gateway_ws_protocol::GATEWAY_WS_ERROR_CODE_INVALID_PAYLOAD,
                                "gateway websocket binary frame must be UTF-8 encoded JSON text",
                            ),
                        };
                        if sender
                            .send(gateway_ws_message_from_frame(&response))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    WsMessage::Ping(payload) => {
                        if sender.send(WsMessage::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    WsMessage::Pong(_) => {}
                    WsMessage::Close(_) => break,
                }
            }
            _ = heartbeat.tick() => {
                if sender.send(WsMessage::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
                let heartbeat_frame = crate::gateway_ws_protocol::build_gateway_ws_response_frame(
                    GATEWAY_WS_HEARTBEAT_REQUEST_ID,
                    "gateway.heartbeat",
                    json!({
                        "ts_unix_ms": current_unix_timestamp_ms(),
                    }),
                );
                if sender
                    .send(gateway_ws_message_from_frame(&heartbeat_frame))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }
    }
}
