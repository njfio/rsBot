//! Gateway WebSocket and dashboard stream handlers.

use super::*;

pub(super) async fn handle_gateway_ws_upgrade(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    websocket: WebSocketUpgrade,
) -> Response {
    let principal = match authorize_gateway_request(&state, &headers) {
        Ok(principal) => principal,
        Err(error) => return error.into_response(),
    };
    if let Err(error) = enforce_gateway_rate_limit(&state, principal.as_str()) {
        return error.into_response();
    }

    websocket
        .on_upgrade(move |socket| run_gateway_ws_connection(state, socket, principal))
        .into_response()
}

pub(super) async fn run_dashboard_stream_loop(
    state: Arc<GatewayOpenResponsesServerState>,
    sender: mpsc::UnboundedSender<Event>,
    reconnect_event_id: Option<String>,
) {
    if let Some(last_event_id) = reconnect_event_id {
        let reset_payload = json!({
            "schema_version": 1,
            "reset": true,
            "last_event_id": last_event_id,
            "reason": "history_not_retained_request_full_snapshot",
        });
        let reset = Event::default()
            .id(format!("dashboard-{}", state.next_sequence()))
            .event("dashboard.reset")
            .data(reset_payload.to_string());
        if sender.send(reset).is_err() {
            return;
        }
    }

    let mut last_snapshot_payload = String::new();
    loop {
        let snapshot = collect_gateway_dashboard_snapshot(&state.config.state_dir);
        let payload_value = match serde_json::to_value(&snapshot) {
            Ok(payload) => payload,
            Err(error) => json!({
                "schema_version": 1,
                "generated_unix_ms": current_unix_timestamp_ms(),
                "error": "dashboard_snapshot_serialize_failed",
                "message": error.to_string(),
            }),
        };
        let payload_string = payload_value.to_string();
        if payload_string != last_snapshot_payload {
            let snapshot_event = Event::default()
                .id(format!("dashboard-{}", state.next_sequence()))
                .event("dashboard.snapshot")
                .data(payload_string.clone());
            if sender.send(snapshot_event).is_err() {
                return;
            }
            last_snapshot_payload = payload_string;
        }
        tokio::time::sleep(Duration::from_millis(750)).await;
    }
}
