//! Core OpenResponses gateway request/response/error types used across handlers and translation.

use std::collections::BTreeMap;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Error payload mapped to OpenAI-compatible HTTP response envelope.
#[derive(Debug)]
pub(super) struct OpenResponsesApiError {
    pub(super) status: StatusCode,
    pub(super) code: &'static str,
    pub(super) message: String,
}

impl OpenResponsesApiError {
    pub(super) fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    pub(super) fn bad_request(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, message)
    }

    pub(super) fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "missing or invalid bearer token",
        )
    }

    pub(super) fn forbidden(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, code, message)
    }

    pub(super) fn not_found(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, code, message)
    }

    pub(super) fn payload_too_large(message: impl Into<String>) -> Self {
        Self::new(StatusCode::PAYLOAD_TOO_LARGE, "input_too_large", message)
    }

    pub(super) fn timeout(message: impl Into<String>) -> Self {
        Self::new(StatusCode::REQUEST_TIMEOUT, "request_timeout", message)
    }

    pub(super) fn gateway_failure(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_GATEWAY, "gateway_runtime_error", message)
    }

    pub(super) fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", message)
    }
}

impl IntoResponse for OpenResponsesApiError {
    fn into_response(self) -> Response {
        let error_type = if self.status.is_client_error() {
            "invalid_request_error"
        } else {
            "server_error"
        };
        (
            self.status,
            Json(json!({
                "error": {
                    "type": error_type,
                    "code": self.code,
                    "message": self.message,
                }
            })),
        )
            .into_response()
    }
}

/// Gateway request body for OpenResponses-compatible chat invocation.
#[derive(Debug, Deserialize)]
pub(super) struct OpenResponsesRequest {
    pub(super) model: Option<String>,
    #[serde(default)]
    pub(super) input: Value,
    #[serde(default)]
    pub(super) stream: bool,
    pub(super) instructions: Option<String>,
    #[serde(default)]
    pub(super) metadata: Value,
    #[serde(default)]
    pub(super) conversation: Option<String>,
    #[serde(default, rename = "previous_response_id")]
    pub(super) previous_response_id: Option<String>,
    #[serde(flatten)]
    pub(super) extra: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GatewayAuthSessionRequest {
    pub(super) password: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct GatewaySessionAppendRequest {
    pub(super) role: String,
    pub(super) content: String,
    #[serde(default)]
    pub(super) policy_gate: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GatewaySessionResetRequest {
    #[serde(default)]
    pub(super) policy_gate: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GatewayMemoryUpdateRequest {
    pub(super) content: String,
    #[serde(default)]
    pub(super) policy_gate: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub(super) struct GatewayMemoryGraphQuery {
    #[serde(default)]
    pub(super) max_nodes: Option<usize>,
    #[serde(default)]
    pub(super) min_edge_weight: Option<f64>,
    #[serde(default)]
    pub(super) relation_types: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GatewayUiTelemetryRequest {
    pub(super) view: String,
    pub(super) action: String,
    #[serde(default)]
    pub(super) session_key: Option<String>,
    #[serde(default)]
    pub(super) reason_code: Option<String>,
    #[serde(default)]
    pub(super) metadata: Value,
}

#[derive(Debug, Deserialize)]
pub(super) struct GatewayExternalCodingAgentSessionOpenRequest {
    pub(super) workspace_id: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct GatewayExternalCodingAgentMessageRequest {
    pub(super) message: String,
}

#[derive(Debug, Deserialize, Default)]
pub(super) struct GatewayExternalCodingAgentFollowupsDrainRequest {
    #[serde(default)]
    pub(super) limit: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
pub(super) struct GatewayExternalCodingAgentStreamQuery {
    #[serde(default)]
    pub(super) after_sequence_id: Option<u64>,
    #[serde(default)]
    pub(super) limit: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
pub(super) struct GatewayExternalCodingAgentReapRequest {
    #[serde(default)]
    pub(super) now_unix_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
pub(super) struct GatewayAuthSessionResponse {
    pub(super) access_token: String,
    pub(super) token_type: &'static str,
    pub(super) expires_unix_ms: u64,
    pub(super) expires_in_seconds: u64,
}

#[derive(Debug, Serialize)]
pub(super) struct GatewayMemoryGraphNode {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) category: String,
    pub(super) weight: f64,
    pub(super) size: f64,
}

#[derive(Debug, Serialize)]
pub(super) struct GatewayMemoryGraphEdge {
    pub(super) id: String,
    pub(super) source: String,
    pub(super) target: String,
    pub(super) relation_type: String,
    pub(super) weight: f64,
}

#[derive(Debug, Serialize)]
pub(super) struct GatewayMemoryGraphFilterSummary {
    pub(super) max_nodes: usize,
    pub(super) min_edge_weight: f64,
    pub(super) relation_types: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct GatewayMemoryGraphResponse {
    pub(super) session_key: String,
    pub(super) path: String,
    pub(super) exists: bool,
    pub(super) bytes: usize,
    pub(super) node_count: usize,
    pub(super) edge_count: usize,
    pub(super) nodes: Vec<GatewayMemoryGraphNode>,
    pub(super) edges: Vec<GatewayMemoryGraphEdge>,
    pub(super) filters: GatewayMemoryGraphFilterSummary,
}

#[derive(Debug)]
pub(super) struct OpenResponsesPrompt {
    pub(super) prompt: String,
    pub(super) session_key: String,
    pub(super) ignored_fields: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct OpenResponsesUsageSummary {
    pub(super) input_tokens: u64,
    pub(super) output_tokens: u64,
    pub(super) total_tokens: u64,
}

#[derive(Debug, Serialize)]
pub(super) struct OpenResponsesOutputTextItem {
    #[serde(rename = "type")]
    pub(super) kind: &'static str,
    pub(super) text: String,
}

#[derive(Debug, Serialize)]
pub(super) struct OpenResponsesOutputItem {
    pub(super) id: String,
    #[serde(rename = "type")]
    pub(super) kind: &'static str,
    pub(super) role: &'static str,
    pub(super) content: Vec<OpenResponsesOutputTextItem>,
}

#[derive(Debug, Serialize)]
pub(super) struct OpenResponsesUsage {
    pub(super) input_tokens: u64,
    pub(super) output_tokens: u64,
    pub(super) total_tokens: u64,
}

/// Normalized OpenResponses response payload returned by gateway runtime.
#[derive(Debug, Serialize)]
pub(super) struct OpenResponsesResponse {
    pub(super) id: String,
    pub(super) object: &'static str,
    pub(super) created: u64,
    pub(super) status: &'static str,
    pub(super) model: String,
    pub(super) output: Vec<OpenResponsesOutputItem>,
    pub(super) output_text: String,
    pub(super) usage: OpenResponsesUsage,
    pub(super) ignored_fields: Vec<String>,
}

#[derive(Debug)]
pub(super) struct OpenResponsesExecutionResult {
    pub(super) response: OpenResponsesResponse,
}

/// Server-sent-event frame variants emitted by streaming endpoints.
#[derive(Debug)]
pub(super) enum SseFrame {
    Json { event: &'static str, payload: Value },
    Done,
}

impl SseFrame {
    pub(super) fn into_event(self) -> axum::response::sse::Event {
        match self {
            Self::Json { event, payload } => axum::response::sse::Event::default()
                .event(event)
                .data(payload.to_string()),
            Self::Done => axum::response::sse::Event::default()
                .event("done")
                .data("[DONE]"),
        }
    }
}
