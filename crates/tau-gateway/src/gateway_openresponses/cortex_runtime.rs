//! Cortex admin chat endpoint handlers.
//!
//! This module provides a deterministic SSE contract for the initial Cortex
//! admin chat API foundation.

use std::convert::Infallible;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::json;
use tau_core::current_unix_timestamp_ms;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

use super::{
    authorize_dashboard_request, parse_gateway_json_body, GatewayOpenResponsesServerState,
    OpenResponsesApiError, SseFrame,
};

#[derive(Debug, Deserialize)]
struct GatewayCortexChatRequest {
    input: String,
}

pub(super) async fn handle_cortex_chat(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(error) = authorize_dashboard_request(&state, &headers) {
        return error.into_response();
    }

    let request = match parse_gateway_json_body::<GatewayCortexChatRequest>(&body) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };
    let normalized_input = request.input.trim();
    if normalized_input.is_empty() {
        return OpenResponsesApiError::bad_request(
            "invalid_cortex_input",
            "input must be non-empty",
        )
        .into_response();
    }

    let response_id = format!("cortex_{}", state.next_response_id());
    let created_unix_ms = current_unix_timestamp_ms();
    let output_text = render_cortex_output_text(normalized_input, state.config.model.as_str());

    let (tx, rx) = mpsc::unbounded_channel::<SseFrame>();
    let _ = tx.send(SseFrame::Json {
        event: "cortex.response.created",
        payload: json!({
            "schema_version": 1,
            "response_id": response_id,
            "created_unix_ms": created_unix_ms,
        }),
    });
    let _ = tx.send(SseFrame::Json {
        event: "cortex.response.output_text.delta",
        payload: json!({
            "response_id": response_id,
            "delta": output_text,
        }),
    });
    let _ = tx.send(SseFrame::Json {
        event: "cortex.response.output_text.done",
        payload: json!({
            "response_id": response_id,
            "text": output_text,
        }),
    });
    let _ = tx.send(SseFrame::Done);
    drop(tx);

    let stream =
        UnboundedReceiverStream::new(rx).map(|frame| Ok::<Event, Infallible>(frame.into_event()));
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

fn render_cortex_output_text(input: &str, model: &str) -> String {
    format!(
        "Cortex admin foundation active. Received {} characters. Active model: {}.",
        input.chars().count(),
        model
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_render_cortex_output_text_includes_char_count_and_model() {
        let output = render_cortex_output_text("hello", "openai/gpt-4o-mini");
        assert!(output.contains("Received 5 characters"));
        assert!(output.contains("openai/gpt-4o-mini"));
    }
}
