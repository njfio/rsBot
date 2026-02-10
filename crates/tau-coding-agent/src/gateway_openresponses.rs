use std::collections::{BTreeMap, BTreeSet};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{header::AUTHORIZATION, HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tau_agent_core::{Agent, AgentConfig, AgentEvent};
use tau_ai::{LlmClient, Message, MessageRole, StreamDeltaHandler};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::StreamExt;

use crate::{current_unix_timestamp, persist_messages, SessionRuntime, SessionStore};

const OPENRESPONSES_ENDPOINT: &str = "/v1/responses";
const WEBCHAT_ENDPOINT: &str = "/webchat";
const GATEWAY_STATUS_ENDPOINT: &str = "/gateway/status";
const DEFAULT_SESSION_KEY: &str = "default";
const INPUT_BODY_SIZE_MULTIPLIER: usize = 8;

#[derive(Clone)]
pub(crate) struct GatewayOpenResponsesServerConfig {
    pub(crate) client: Arc<dyn LlmClient>,
    pub(crate) model: String,
    pub(crate) system_prompt: String,
    pub(crate) max_turns: usize,
    pub(crate) tool_policy: crate::tools::ToolPolicy,
    pub(crate) turn_timeout_ms: u64,
    pub(crate) session_lock_wait_ms: u64,
    pub(crate) session_lock_stale_ms: u64,
    pub(crate) state_dir: PathBuf,
    pub(crate) bind: String,
    pub(crate) auth_token: String,
    pub(crate) max_input_chars: usize,
}

#[derive(Clone)]
struct GatewayOpenResponsesServerState {
    config: GatewayOpenResponsesServerConfig,
    response_sequence: Arc<AtomicU64>,
}

impl GatewayOpenResponsesServerState {
    fn new(config: GatewayOpenResponsesServerConfig) -> Self {
        Self {
            config,
            response_sequence: Arc::new(AtomicU64::new(0)),
        }
    }

    fn next_sequence(&self) -> u64 {
        self.response_sequence.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn next_response_id(&self) -> String {
        format!("resp_{:016x}", self.next_sequence())
    }

    fn next_output_message_id(&self) -> String {
        format!("msg_{:016x}", self.next_sequence())
    }
}

#[derive(Debug)]
struct OpenResponsesApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl OpenResponsesApiError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    fn bad_request(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, message)
    }

    fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "missing or invalid bearer token",
        )
    }

    fn payload_too_large(message: impl Into<String>) -> Self {
        Self::new(StatusCode::PAYLOAD_TOO_LARGE, "input_too_large", message)
    }

    fn timeout(message: impl Into<String>) -> Self {
        Self::new(StatusCode::REQUEST_TIMEOUT, "request_timeout", message)
    }

    fn gateway_failure(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_GATEWAY, "gateway_runtime_error", message)
    }

    fn internal(message: impl Into<String>) -> Self {
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

#[derive(Debug, Deserialize)]
struct OpenResponsesRequest {
    #[allow(dead_code)]
    model: Option<String>,
    #[serde(default)]
    input: Value,
    #[serde(default)]
    stream: bool,
    instructions: Option<String>,
    #[serde(default)]
    metadata: Value,
    #[serde(default)]
    conversation: Option<String>,
    #[serde(default, rename = "previous_response_id")]
    previous_response_id: Option<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Debug)]
struct OpenResponsesPrompt {
    prompt: String,
    session_key: String,
    ignored_fields: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct OpenResponsesUsageSummary {
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
}

#[derive(Debug, Serialize)]
struct OpenResponsesOutputTextItem {
    #[serde(rename = "type")]
    kind: &'static str,
    text: String,
}

#[derive(Debug, Serialize)]
struct OpenResponsesOutputItem {
    id: String,
    #[serde(rename = "type")]
    kind: &'static str,
    role: &'static str,
    content: Vec<OpenResponsesOutputTextItem>,
}

#[derive(Debug, Serialize)]
struct OpenResponsesUsage {
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
}

#[derive(Debug, Serialize)]
struct OpenResponsesResponse {
    id: String,
    object: &'static str,
    created: u64,
    status: &'static str,
    model: String,
    output: Vec<OpenResponsesOutputItem>,
    output_text: String,
    usage: OpenResponsesUsage,
    ignored_fields: Vec<String>,
}

#[derive(Debug)]
struct OpenResponsesExecutionResult {
    response: OpenResponsesResponse,
}

#[derive(Debug)]
enum SseFrame {
    Json { event: &'static str, payload: Value },
    Done,
}

impl SseFrame {
    fn into_event(self) -> Event {
        match self {
            Self::Json { event, payload } => {
                Event::default().event(event).data(payload.to_string())
            }
            Self::Done => Event::default().event("done").data("[DONE]"),
        }
    }
}

pub(crate) async fn run_gateway_openresponses_server(
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

    println!(
        "gateway openresponses server listening: endpoint={} addr={} state_dir={}",
        OPENRESPONSES_ENDPOINT,
        local_addr,
        config.state_dir.display()
    );

    let state_dir = config.state_dir.clone();
    let state = Arc::new(GatewayOpenResponsesServerState::new(config));
    let app = build_gateway_openresponses_router(state);
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .context("gateway openresponses server exited unexpectedly")?;

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

fn build_gateway_openresponses_router(state: Arc<GatewayOpenResponsesServerState>) -> Router {
    Router::new()
        .route(OPENRESPONSES_ENDPOINT, post(handle_openresponses))
        .route(WEBCHAT_ENDPOINT, get(handle_webchat_page))
        .route(GATEWAY_STATUS_ENDPOINT, get(handle_gateway_status))
        .with_state(state)
}

async fn handle_webchat_page() -> Html<String> {
    Html(render_gateway_webchat_page())
}

async fn handle_gateway_status(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
) -> Response {
    if !authorization_is_valid(&headers, &state.config.auth_token) {
        return OpenResponsesApiError::unauthorized().into_response();
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

    (
        StatusCode::OK,
        Json(json!({
            "service": service_report,
            "gateway": {
                "responses_endpoint": OPENRESPONSES_ENDPOINT,
                "webchat_endpoint": WEBCHAT_ENDPOINT,
                "status_endpoint": GATEWAY_STATUS_ENDPOINT,
                "state_dir": state.config.state_dir.display().to_string(),
                "model": state.config.model,
            }
        })),
    )
        .into_response()
}

async fn handle_openresponses(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !authorization_is_valid(&headers, &state.config.auth_token) {
        return OpenResponsesApiError::unauthorized().into_response();
    }

    let body_limit = state
        .config
        .max_input_chars
        .saturating_mul(INPUT_BODY_SIZE_MULTIPLIER)
        .max(state.config.max_input_chars);
    if body.len() > body_limit {
        return OpenResponsesApiError::payload_too_large(format!(
            "request body exceeds max size of {} bytes",
            body_limit
        ))
        .into_response();
    }

    let request = match serde_json::from_slice::<OpenResponsesRequest>(&body) {
        Ok(request) => request,
        Err(error) => {
            return OpenResponsesApiError::bad_request(
                "malformed_json",
                format!("failed to parse request body: {error}"),
            )
            .into_response();
        }
    };

    if request.stream {
        return stream_openresponses(state, request).await;
    }

    match execute_openresponses_request(state, request, None).await {
        Ok(result) => (StatusCode::OK, Json(result.response)).into_response(),
        Err(error) => error.into_response(),
    }
}

async fn stream_openresponses(
    state: Arc<GatewayOpenResponsesServerState>,
    request: OpenResponsesRequest,
) -> Response {
    let (tx, rx) = mpsc::unbounded_channel::<SseFrame>();
    tokio::spawn(async move {
        match execute_openresponses_request(state, request, Some(tx.clone())).await {
            Ok(result) => {
                let response = result.response;
                let _ = tx.send(SseFrame::Json {
                    event: "response.output_text.done",
                    payload: json!({
                        "type": "response.output_text.done",
                        "response_id": response.id,
                        "text": response.output_text,
                    }),
                });
                let _ = tx.send(SseFrame::Json {
                    event: "response.completed",
                    payload: json!({
                        "type": "response.completed",
                        "response": response,
                    }),
                });
                let _ = tx.send(SseFrame::Done);
            }
            Err(error) => {
                let _ = tx.send(SseFrame::Json {
                    event: "response.failed",
                    payload: json!({
                        "type": "response.failed",
                        "error": {
                            "code": error.code,
                            "message": error.message,
                        }
                    }),
                });
                let _ = tx.send(SseFrame::Done);
            }
        }
    });

    let stream =
        UnboundedReceiverStream::new(rx).map(|frame| Ok::<Event, Infallible>(frame.into_event()));
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

async fn execute_openresponses_request(
    state: Arc<GatewayOpenResponsesServerState>,
    request: OpenResponsesRequest,
    stream_sender: Option<mpsc::UnboundedSender<SseFrame>>,
) -> Result<OpenResponsesExecutionResult, OpenResponsesApiError> {
    let mut translated = translate_openresponses_request(&request, state.config.max_input_chars)?;
    if request.model.is_some() {
        translated.ignored_fields.push("model".to_string());
    }

    let response_id = state.next_response_id();
    let created = current_unix_timestamp();

    if let Some(sender) = &stream_sender {
        let _ = sender.send(SseFrame::Json {
            event: "response.created",
            payload: json!({
                "type": "response.created",
                "response": {
                    "id": response_id,
                    "object": "response",
                    "status": "in_progress",
                    "model": state.config.model,
                    "created": created,
                }
            }),
        });
    }

    let mut agent = Agent::new(
        state.config.client.clone(),
        AgentConfig {
            model: state.config.model.clone(),
            system_prompt: state.config.system_prompt.clone(),
            max_turns: state.config.max_turns,
            temperature: Some(0.0),
            max_tokens: None,
        },
    );
    crate::tools::register_builtin_tools(&mut agent, state.config.tool_policy.clone());

    let usage = Arc::new(Mutex::new(OpenResponsesUsageSummary::default()));
    agent.subscribe({
        let usage = usage.clone();
        move |event| {
            if let AgentEvent::TurnEnd {
                usage: turn_usage, ..
            } = event
            {
                if let Ok(mut guard) = usage.lock() {
                    guard.input_tokens = guard.input_tokens.saturating_add(turn_usage.input_tokens);
                    guard.output_tokens =
                        guard.output_tokens.saturating_add(turn_usage.output_tokens);
                    guard.total_tokens = guard.total_tokens.saturating_add(turn_usage.total_tokens);
                }
            }
        }
    });

    let session_path = gateway_session_path(&state.config.state_dir, &translated.session_key);
    let mut session_runtime = Some(
        initialize_gateway_session_runtime(
            &session_path,
            &state.config.system_prompt,
            state.config.session_lock_wait_ms,
            state.config.session_lock_stale_ms,
            &mut agent,
        )
        .map_err(|error| {
            OpenResponsesApiError::internal(format!(
                "failed to initialize gateway session runtime: {error}"
            ))
        })?,
    );

    let start_index = agent.messages().len();
    let stream_handler = stream_sender.as_ref().map(|sender| {
        let sender = sender.clone();
        let response_id = response_id.clone();
        Arc::new(move |delta: String| {
            if delta.is_empty() {
                return;
            }
            let _ = sender.send(SseFrame::Json {
                event: "response.output_text.delta",
                payload: json!({
                    "type": "response.output_text.delta",
                    "response_id": response_id,
                    "delta": delta,
                }),
            });
        }) as StreamDeltaHandler
    });

    let prompt_result = if state.config.turn_timeout_ms == 0 {
        agent
            .prompt_with_stream(&translated.prompt, stream_handler)
            .await
    } else {
        match tokio::time::timeout(
            Duration::from_millis(state.config.turn_timeout_ms),
            agent.prompt_with_stream(&translated.prompt, stream_handler),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => {
                return Err(OpenResponsesApiError::timeout(
                    "response generation timed out before completion",
                ));
            }
        }
    };

    let new_messages = prompt_result.map_err(|error| {
        OpenResponsesApiError::gateway_failure(format!("gateway runtime failed: {error}"))
    })?;
    persist_messages(&mut session_runtime, &new_messages).map_err(|error| {
        OpenResponsesApiError::internal(format!(
            "failed to persist gateway session messages: {error}"
        ))
    })?;

    let output_text = collect_assistant_reply(&agent.messages()[start_index..]);
    let usage = usage
        .lock()
        .map_err(|_| OpenResponsesApiError::internal("prompt usage lock is poisoned"))?
        .clone();

    let mut ignored = BTreeSet::new();
    for field in translated.ignored_fields {
        if !field.trim().is_empty() {
            ignored.insert(field);
        }
    }

    let response = OpenResponsesResponse {
        id: response_id,
        object: "response",
        created,
        status: "completed",
        model: state.config.model.clone(),
        output: vec![OpenResponsesOutputItem {
            id: state.next_output_message_id(),
            kind: "message",
            role: "assistant",
            content: vec![OpenResponsesOutputTextItem {
                kind: "output_text",
                text: output_text.clone(),
            }],
        }],
        output_text,
        usage: OpenResponsesUsage {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            total_tokens: usage.total_tokens,
        },
        ignored_fields: ignored.into_iter().collect(),
    };

    Ok(OpenResponsesExecutionResult { response })
}

fn translate_openresponses_request(
    request: &OpenResponsesRequest,
    max_input_chars: usize,
) -> Result<OpenResponsesPrompt, OpenResponsesApiError> {
    let mut ignored_fields = request.extra.keys().cloned().collect::<Vec<_>>();
    ignored_fields.sort();

    let mut segments = Vec::new();
    if let Some(instructions) = non_empty_trimmed(request.instructions.as_deref()) {
        segments.push(format!("System instructions:\n{instructions}"));
    }

    if let Some(previous_response_id) = non_empty_trimmed(request.previous_response_id.as_deref()) {
        segments.push(format!(
            "Continuation context (previous_response_id):\n{previous_response_id}"
        ));
    }

    let mut extracted = 0usize;
    extract_openresponses_input_segments(
        &request.input,
        &mut segments,
        &mut extracted,
        &mut ignored_fields,
    )?;

    if extracted == 0 {
        return Err(OpenResponsesApiError::bad_request(
            "missing_input",
            "input must include at least one textual message or function_call_output item",
        ));
    }

    let prompt = segments
        .iter()
        .filter_map(|segment| {
            let trimmed = segment.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    if prompt.is_empty() {
        return Err(OpenResponsesApiError::bad_request(
            "missing_input",
            "input did not contain usable text",
        ));
    }

    if prompt.chars().count() > max_input_chars {
        return Err(OpenResponsesApiError::payload_too_large(format!(
            "translated input exceeds max {} characters",
            max_input_chars
        )));
    }

    let session_seed = metadata_string(&request.metadata, "session_id")
        .or_else(|| non_empty_trimmed(request.conversation.as_deref()))
        .or_else(|| non_empty_trimmed(request.previous_response_id.as_deref()))
        .unwrap_or(DEFAULT_SESSION_KEY);

    Ok(OpenResponsesPrompt {
        prompt,
        session_key: sanitize_session_key(session_seed),
        ignored_fields,
    })
}

fn extract_openresponses_input_segments(
    input: &Value,
    segments: &mut Vec<String>,
    extracted: &mut usize,
    ignored_fields: &mut Vec<String>,
) -> Result<(), OpenResponsesApiError> {
    match input {
        Value::Null => Err(OpenResponsesApiError::bad_request(
            "missing_input",
            "input is required",
        )),
        Value::String(text) => {
            let text = text.trim();
            if !text.is_empty() {
                segments.push(format!("User:\n{text}"));
                *extracted = extracted.saturating_add(1);
            }
            Ok(())
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                extract_openresponses_item(item, index, segments, extracted, ignored_fields)?;
            }
            Ok(())
        }
        Value::Object(_) => {
            extract_openresponses_item(input, 0, segments, extracted, ignored_fields)
        }
        _ => Err(OpenResponsesApiError::bad_request(
            "invalid_input",
            "input must be a string, object, or array",
        )),
    }
}

fn extract_openresponses_item(
    item: &Value,
    index: usize,
    segments: &mut Vec<String>,
    extracted: &mut usize,
    ignored_fields: &mut Vec<String>,
) -> Result<(), OpenResponsesApiError> {
    match item {
        Value::String(text) => {
            let text = text.trim();
            if !text.is_empty() {
                segments.push(format!("User:\n{text}"));
                *extracted = extracted.saturating_add(1);
            }
            Ok(())
        }
        Value::Object(map) => {
            let item_type = map.get("type").and_then(Value::as_str).unwrap_or_default();
            if item_type == "function_call_output" {
                let output = stringify_output(map.get("output").unwrap_or(&Value::Null));
                if output.is_empty() {
                    return Err(OpenResponsesApiError::bad_request(
                        "invalid_function_call_output",
                        format!(
                            "input[{index}] function_call_output item requires non-empty output"
                        ),
                    ));
                }
                let call_id = map
                    .get("call_id")
                    .or_else(|| map.get("id"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("unknown");
                segments.push(format!("Function output (call_id={call_id}):\n{output}"));
                *extracted = extracted.saturating_add(1);
                return Ok(());
            }

            if item_type == "message" || map.contains_key("role") || map.contains_key("content") {
                let role = map.get("role").and_then(Value::as_str).unwrap_or("user");
                let text = extract_message_content_text(map.get("content"));
                if !text.is_empty() {
                    segments.push(format!("{}:\n{}", role_label(role), text));
                    *extracted = extracted.saturating_add(1);
                } else {
                    ignored_fields.push(format!("input[{index}].content"));
                }
                return Ok(());
            }

            ignored_fields.push(format!("input[{index}]"));
            Ok(())
        }
        _ => {
            ignored_fields.push(format!("input[{index}]"));
            Ok(())
        }
    }
}

fn extract_message_content_text(content: Option<&Value>) -> String {
    let Some(content) = content else {
        return String::new();
    };

    match content {
        Value::String(text) => text.trim().to_string(),
        Value::Array(parts) => {
            let mut segments = Vec::new();
            for part in parts {
                if let Some(text) = extract_message_content_part(part) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        segments.push(trimmed.to_string());
                    }
                }
            }
            segments.join("\n")
        }
        Value::Object(_) => extract_message_content_part(content).unwrap_or_default(),
        _ => String::new(),
    }
}

fn extract_message_content_part(part: &Value) -> Option<String> {
    match part {
        Value::String(text) => Some(text.to_string()),
        Value::Object(map) => {
            let part_type = map.get("type").and_then(Value::as_str).unwrap_or("text");
            match part_type {
                "input_text" | "output_text" | "text" => map
                    .get("text")
                    .and_then(Value::as_str)
                    .map(|value| value.to_string()),
                "function_call_output" => {
                    let output = stringify_output(map.get("output").unwrap_or(&Value::Null));
                    if output.trim().is_empty() {
                        return None;
                    }
                    let call_id = map
                        .get("call_id")
                        .or_else(|| map.get("id"))
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .unwrap_or("unknown");
                    Some(format!("Function output (call_id={call_id}):\n{output}"))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn stringify_output(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.trim().to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

fn role_label(role: &str) -> &'static str {
    match role.trim().to_ascii_lowercase().as_str() {
        "assistant" => "Assistant context",
        "system" => "System context",
        "tool" => "Tool context",
        _ => "User",
    }
}

fn metadata_string<'a>(metadata: &'a Value, key: &str) -> Option<&'a str> {
    metadata
        .as_object()?
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn non_empty_trimmed(raw: Option<&str>) -> Option<&str> {
    raw.map(str::trim).filter(|value| !value.is_empty())
}

fn sanitize_session_key(raw: &str) -> String {
    let mut normalized = String::new();
    for ch in raw.trim().chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            normalized.push(ch);
        } else {
            normalized.push('_');
        }
    }
    let normalized = normalized.trim_matches('_').to_string();
    if normalized.is_empty() {
        DEFAULT_SESSION_KEY.to_string()
    } else {
        normalized
    }
}

fn gateway_session_path(state_dir: &Path, session_key: &str) -> PathBuf {
    state_dir
        .join("openresponses")
        .join("sessions")
        .join(format!("{session_key}.jsonl"))
}

fn initialize_gateway_session_runtime(
    session_path: &Path,
    system_prompt: &str,
    lock_wait_ms: u64,
    lock_stale_ms: u64,
    agent: &mut Agent,
) -> Result<SessionRuntime> {
    if let Some(parent) = session_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let mut store = SessionStore::load(session_path)?;
    store.set_lock_policy(lock_wait_ms.max(1), lock_stale_ms);
    let active_head = store.ensure_initialized(system_prompt)?;
    let lineage = store.lineage_messages(active_head)?;
    if !lineage.is_empty() {
        agent.replace_messages(lineage);
    }
    Ok(SessionRuntime { store, active_head })
}

fn collect_assistant_reply(messages: &[Message]) -> String {
    let content = messages
        .iter()
        .filter(|message| message.role == MessageRole::Assistant)
        .map(Message::text_content)
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if content.trim().is_empty() {
        "I couldn't generate a textual response for this request.".to_string()
    } else {
        content
    }
}

fn render_gateway_webchat_page() -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Tau Gateway Webchat</title>
  <style>
    :root {{
      color-scheme: light;
      font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
    }}
    body {{
      margin: 0;
      background: linear-gradient(160deg, #f4f6f8 0%, #eef2f7 100%);
      color: #13232f;
    }}
    .container {{
      max-width: 980px;
      margin: 0 auto;
      padding: 1.5rem;
    }}
    h1 {{
      margin: 0 0 0.5rem 0;
      font-size: 1.5rem;
    }}
    p {{
      margin: 0.25rem 0 1rem 0;
      color: #3a4f5f;
    }}
    .grid {{
      display: grid;
      gap: 1rem;
      grid-template-columns: 1fr;
    }}
    .panel {{
      background: #ffffff;
      border: 1px solid #d2dde6;
      border-radius: 12px;
      padding: 1rem;
      box-shadow: 0 8px 20px rgba(12, 25, 38, 0.06);
    }}
    label {{
      display: block;
      font-size: 0.85rem;
      margin-bottom: 0.25rem;
      color: #375062;
    }}
    input[type="text"], textarea {{
      width: 100%;
      box-sizing: border-box;
      border: 1px solid #b8c9d6;
      border-radius: 8px;
      padding: 0.55rem 0.7rem;
      font-size: 0.95rem;
      background: #fbfdff;
      color: #13232f;
    }}
    textarea {{
      min-height: 150px;
      resize: vertical;
    }}
    .row {{
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
      gap: 0.8rem;
      margin-bottom: 0.8rem;
    }}
    .actions {{
      display: flex;
      gap: 0.5rem;
      flex-wrap: wrap;
      margin-top: 0.8rem;
    }}
    button {{
      border: 0;
      border-radius: 8px;
      background: #0f7d5f;
      color: #ffffff;
      padding: 0.55rem 0.9rem;
      font-weight: 600;
      cursor: pointer;
    }}
    button.secondary {{
      background: #3f5f74;
    }}
    button:disabled {{
      cursor: wait;
      opacity: 0.6;
    }}
    .checkbox {{
      display: inline-flex;
      align-items: center;
      gap: 0.4rem;
      margin-top: 0.2rem;
      color: #274355;
      font-size: 0.9rem;
    }}
    pre {{
      margin: 0;
      background: #0f1f2b;
      color: #d9ecf7;
      border-radius: 10px;
      padding: 0.8rem;
      overflow: auto;
      max-height: 300px;
      white-space: pre-wrap;
      word-break: break-word;
      font-size: 0.85rem;
    }}
    @media (min-width: 900px) {{
      .grid {{
        grid-template-columns: 1.4fr 1fr;
      }}
    }}
  </style>
</head>
<body>
  <main class="container">
    <h1>Tau Gateway Webchat</h1>
    <p>Operator webchat for the OpenResponses gateway runtime.</p>
    <div class="grid">
      <section class="panel">
        <div class="row">
          <div>
            <label for="authToken">Bearer token</label>
            <input id="authToken" type="text" autocomplete="off" placeholder="gateway auth token" />
          </div>
          <div>
            <label for="sessionKey">Session key</label>
            <input id="sessionKey" type="text" autocomplete="off" value="{default_session_key}" />
          </div>
        </div>
        <label class="checkbox">
          <input id="stream" type="checkbox" checked />
          Stream response (SSE)
        </label>
        <div style="margin-top: 0.8rem;">
          <label for="prompt">Prompt</label>
          <textarea id="prompt" placeholder="Ask Tau through the gateway..."></textarea>
        </div>
        <div class="actions">
          <button id="send">Send</button>
          <button id="refreshStatus" class="secondary">Refresh status</button>
          <button id="clearOutput" class="secondary">Clear output</button>
        </div>
      </section>
      <section class="panel">
        <h2 style="margin: 0 0 0.5rem 0; font-size: 1rem;">Gateway status</h2>
        <pre id="status">Press "Refresh status" to inspect gateway service state.</pre>
      </section>
    </div>
    <section class="panel" style="margin-top: 1rem;">
      <h2 style="margin: 0 0 0.5rem 0; font-size: 1rem;">Response output</h2>
      <pre id="output">No response yet.</pre>
    </section>
  </main>
  <script>
    const RESPONSES_ENDPOINT = "{responses_endpoint}";
    const STATUS_ENDPOINT = "{status_endpoint}";
    const STORAGE_TOKEN = "tau.gateway.webchat.token";
    const STORAGE_SESSION = "tau.gateway.webchat.session";
    const tokenInput = document.getElementById("authToken");
    const sessionInput = document.getElementById("sessionKey");
    const streamInput = document.getElementById("stream");
    const promptInput = document.getElementById("prompt");
    const outputPre = document.getElementById("output");
    const statusPre = document.getElementById("status");
    const sendButton = document.getElementById("send");
    const refreshButton = document.getElementById("refreshStatus");
    const clearButton = document.getElementById("clearOutput");

    function loadLocalValues() {{
      const storedToken = window.localStorage.getItem(STORAGE_TOKEN);
      const storedSession = window.localStorage.getItem(STORAGE_SESSION);
      if (storedToken) {{
        tokenInput.value = storedToken;
      }}
      if (storedSession) {{
        sessionInput.value = storedSession;
      }}
    }}

    function saveLocalValues() {{
      window.localStorage.setItem(STORAGE_TOKEN, tokenInput.value.trim());
      window.localStorage.setItem(STORAGE_SESSION, sessionInput.value.trim());
    }}

    function authHeaders() {{
      const token = tokenInput.value.trim();
      if (token.length === 0) {{
        return {{}};
      }}
      return {{
        "Authorization": "Bearer " + token
      }};
    }}

    function setOutput(text) {{
      outputPre.textContent = text;
    }}

    function appendOutput(text) {{
      if (outputPre.textContent === "No response yet.") {{
        outputPre.textContent = "";
      }}
      outputPre.textContent += text;
    }}

    function processSseFrame(frame) {{
      if (!frame || frame.trim().length === 0) {{
        return;
      }}
      let eventName = "";
      let data = "";
      const lines = frame.split(/\r?\n/);
      for (const line of lines) {{
        if (line.startsWith("event:")) {{
          eventName = line.slice("event:".length).trim();
        }} else if (line.startsWith("data:")) {{
          data += line.slice("data:".length).trim();
        }}
      }}
      if (data.length === 0 || data === "[DONE]") {{
        return;
      }}
      let payload = null;
      try {{
        payload = JSON.parse(data);
      }} catch (error) {{
        appendOutput("\n[invalid sse payload] " + data + "\n");
        return;
      }}
      if (eventName === "response.output_text.delta") {{
        appendOutput(payload.delta || "");
        return;
      }}
      if (eventName === "response.output_text.done") {{
        appendOutput("\n");
        return;
      }}
      if (eventName === "response.failed") {{
        const message = payload && payload.error ? payload.error.message : "unknown";
        appendOutput("\n[gateway error] " + message + "\n");
      }}
    }}

    async function readSseBody(response) {{
      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";
      while (true) {{
        const result = await reader.read();
        if (result.done) {{
          break;
        }}
        buffer += decoder.decode(result.value, {{ stream: true }});
        while (true) {{
          const splitIndex = buffer.indexOf("\n\n");
          if (splitIndex < 0) {{
            break;
          }}
          const frame = buffer.slice(0, splitIndex);
          buffer = buffer.slice(splitIndex + 2);
          processSseFrame(frame);
        }}
      }}
      if (buffer.trim().length > 0) {{
        processSseFrame(buffer);
      }}
    }}

    async function refreshStatus() {{
      statusPre.textContent = "Loading gateway status...";
      try {{
        const response = await fetch(STATUS_ENDPOINT, {{
          headers: authHeaders()
        }});
        const raw = await response.text();
        if (!response.ok) {{
          statusPre.textContent = "status " + response.status + "\n" + raw;
          return;
        }}
        const payload = JSON.parse(raw);
        statusPre.textContent = JSON.stringify(payload, null, 2);
      }} catch (error) {{
        statusPre.textContent = "status request failed: " + String(error);
      }}
    }}

    async function sendPrompt() {{
      const prompt = promptInput.value.trim();
      const sessionKey = sessionInput.value.trim() || "{default_session_key}";
      if (prompt.length === 0) {{
        setOutput("Prompt is required.");
        return;
      }}
      saveLocalValues();
      sendButton.disabled = true;
      try {{
        setOutput("");
        const payload = {{
          input: prompt,
          stream: streamInput.checked,
          metadata: {{
            session_id: sessionKey
          }}
        }};
        const response = await fetch(RESPONSES_ENDPOINT, {{
          method: "POST",
          headers: Object.assign({{
            "Content-Type": "application/json"
          }}, authHeaders()),
          body: JSON.stringify(payload)
        }});
        if (!response.ok) {{
          setOutput("request failed: status=" + response.status + "\n" + await response.text());
          return;
        }}
        if (streamInput.checked) {{
          await readSseBody(response);
        }} else {{
          const body = await response.json();
          const outputText = typeof body.output_text === "string"
            ? body.output_text
            : JSON.stringify(body, null, 2);
          setOutput(outputText);
        }}
        await refreshStatus();
      }} catch (error) {{
        setOutput("request failed: " + String(error));
      }} finally {{
        sendButton.disabled = false;
      }}
    }}

    sendButton.addEventListener("click", sendPrompt);
    refreshButton.addEventListener("click", refreshStatus);
    clearButton.addEventListener("click", () => setOutput("No response yet."));
    tokenInput.addEventListener("change", saveLocalValues);
    sessionInput.addEventListener("change", saveLocalValues);

    loadLocalValues();
  </script>
</body>
</html>
"#,
        responses_endpoint = OPENRESPONSES_ENDPOINT,
        status_endpoint = GATEWAY_STATUS_ENDPOINT,
        default_session_key = DEFAULT_SESSION_KEY,
    )
}

fn authorization_is_valid(headers: &HeaderMap, auth_token: &str) -> bool {
    let Some(header) = headers.get(AUTHORIZATION) else {
        return false;
    };
    let Ok(raw) = header.to_str() else {
        return false;
    };
    let Some(token) = raw.strip_prefix("Bearer ") else {
        return false;
    };
    token.trim() == auth_token
}

pub(crate) fn validate_gateway_openresponses_bind(bind: &str) -> Result<SocketAddr> {
    bind.parse::<SocketAddr>()
        .with_context(|| format!("invalid gateway socket address '{bind}'"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use reqwest::Client;
    use serde_json::Value;
    use tau_ai::{ChatRequest, ChatResponse, ChatUsage, TauAiError};
    use tempfile::tempdir;

    #[derive(Clone, Default)]
    struct MockGatewayLlmClient {
        request_message_counts: Arc<Mutex<Vec<usize>>>,
    }

    #[async_trait]
    impl LlmClient for MockGatewayLlmClient {
        async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, TauAiError> {
            self.complete_with_stream(request, None).await
        }

        async fn complete_with_stream(
            &self,
            request: ChatRequest,
            on_delta: Option<StreamDeltaHandler>,
        ) -> Result<ChatResponse, TauAiError> {
            let message_count = request.messages.len();
            if let Ok(mut counts) = self.request_message_counts.lock() {
                counts.push(message_count);
            }
            if let Some(handler) = on_delta {
                handler("messages=".to_string());
                handler(message_count.to_string());
            }
            let reply = format!("messages={message_count}");
            Ok(ChatResponse {
                message: Message::assistant_text(reply),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage {
                    input_tokens: message_count as u64,
                    output_tokens: 2,
                    total_tokens: message_count as u64 + 2,
                },
            })
        }
    }

    fn test_state(
        root: &Path,
        max_input_chars: usize,
        token: &str,
    ) -> Arc<GatewayOpenResponsesServerState> {
        Arc::new(GatewayOpenResponsesServerState::new(
            GatewayOpenResponsesServerConfig {
                client: Arc::new(MockGatewayLlmClient::default()),
                model: "openai/gpt-4o-mini".to_string(),
                system_prompt: "You are Tau.".to_string(),
                max_turns: 4,
                tool_policy: crate::tools::ToolPolicy::new(Vec::new()),
                turn_timeout_ms: 0,
                session_lock_wait_ms: 500,
                session_lock_stale_ms: 10_000,
                state_dir: root.join(".tau/gateway"),
                bind: "127.0.0.1:0".to_string(),
                auth_token: token.to_string(),
                max_input_chars,
            },
        ))
    }

    async fn spawn_test_server(
        state: Arc<GatewayOpenResponsesServerState>,
    ) -> Result<(SocketAddr, tokio::task::JoinHandle<()>)> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .context("bind ephemeral listener")?;
        let addr = listener.local_addr().context("resolve listener addr")?;
        let app = build_gateway_openresponses_router(state);
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        Ok((addr, handle))
    }

    #[test]
    fn unit_translate_openresponses_request_supports_item_input_and_function_call_output() {
        let request = OpenResponsesRequest {
            model: None,
            input: json!([
                {
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "Please summarize."}]
                },
                {
                    "type": "function_call_output",
                    "call_id": "call_123",
                    "output": "tool result"
                }
            ]),
            stream: false,
            instructions: Some("be concise".to_string()),
            metadata: json!({"session_id": "issue-42"}),
            conversation: None,
            previous_response_id: None,
            extra: BTreeMap::from([("temperature".to_string(), json!(0.0))]),
        };

        let translated =
            translate_openresponses_request(&request, 10_000).expect("translate request");
        assert!(translated.prompt.contains("System instructions"));
        assert!(translated.prompt.contains("Please summarize."));
        assert!(translated
            .prompt
            .contains("Function output (call_id=call_123):"));
        assert_eq!(translated.session_key, "issue-42");
        assert_eq!(translated.ignored_fields, vec!["temperature".to_string()]);
    }

    #[test]
    fn unit_translate_openresponses_request_rejects_invalid_input_shape() {
        let request = OpenResponsesRequest {
            model: None,
            input: json!(42),
            stream: false,
            instructions: None,
            metadata: json!({}),
            conversation: None,
            previous_response_id: None,
            extra: BTreeMap::new(),
        };

        let error =
            translate_openresponses_request(&request, 1024).expect_err("invalid input should fail");
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.code, "invalid_input");
    }

    #[test]
    fn unit_render_gateway_webchat_page_includes_expected_endpoints() {
        let html = render_gateway_webchat_page();
        assert!(html.contains("Tau Gateway Webchat"));
        assert!(html.contains(OPENRESPONSES_ENDPOINT));
        assert!(html.contains(GATEWAY_STATUS_ENDPOINT));
        assert!(html.contains(DEFAULT_SESSION_KEY));
    }

    #[tokio::test]
    async fn functional_webchat_endpoint_returns_html_shell() {
        let temp = tempdir().expect("tempdir");
        let state = test_state(temp.path(), 10_000, "secret");
        let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

        let client = Client::new();
        let response = client
            .get(format!("http://{addr}{WEBCHAT_ENDPOINT}"))
            .send()
            .await
            .expect("send request");

        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(content_type.contains("text/html"));
        let body = response.text().await.expect("read webchat body");
        assert!(body.contains("Tau Gateway Webchat"));
        assert!(body.contains(OPENRESPONSES_ENDPOINT));
        assert!(body.contains(GATEWAY_STATUS_ENDPOINT));

        handle.abort();
    }

    #[tokio::test]
    async fn functional_openresponses_endpoint_rejects_unauthorized_requests() {
        let temp = tempdir().expect("tempdir");
        let state = test_state(temp.path(), 10_000, "secret");
        let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

        let client = Client::new();
        let response = client
            .post(format!("http://{addr}/v1/responses"))
            .json(&json!({"input":"hello"}))
            .send()
            .await
            .expect("send request");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        handle.abort();
    }

    #[tokio::test]
    async fn functional_openresponses_endpoint_returns_non_stream_response() {
        let temp = tempdir().expect("tempdir");
        let state = test_state(temp.path(), 10_000, "secret");
        let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

        let client = Client::new();
        let response = client
            .post(format!("http://{addr}/v1/responses"))
            .bearer_auth("secret")
            .json(&json!({"input":"hello"}))
            .send()
            .await
            .expect("send request");

        assert_eq!(response.status(), StatusCode::OK);
        let payload = response
            .json::<serde_json::Value>()
            .await
            .expect("parse response json");
        assert_eq!(payload["object"], "response");
        assert_eq!(payload["status"], "completed");
        assert!(payload["output_text"]
            .as_str()
            .unwrap_or_default()
            .contains("messages="));

        handle.abort();
    }

    #[tokio::test]
    async fn functional_openresponses_endpoint_streams_sse_for_stream_true() {
        let temp = tempdir().expect("tempdir");
        let state = test_state(temp.path(), 10_000, "secret");
        let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

        let client = Client::new();
        let response = client
            .post(format!("http://{addr}/v1/responses"))
            .bearer_auth("secret")
            .json(&json!({"input":"hello", "stream": true}))
            .send()
            .await
            .expect("send request");

        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(content_type.contains("text/event-stream"));

        let body = response.text().await.expect("read sse body");
        assert!(body.contains("event: response.created"));
        assert!(body.contains("event: response.output_text.delta"));
        assert!(body.contains("event: response.completed"));
        assert!(body.contains("event: done"));

        handle.abort();
    }

    #[tokio::test]
    async fn integration_openresponses_http_roundtrip_persists_session_state() {
        let temp = tempdir().expect("tempdir");
        let state = test_state(temp.path(), 10_000, "secret");
        let (addr, handle) = spawn_test_server(state.clone())
            .await
            .expect("spawn server");

        let client = Client::new();
        let response_one = client
            .post(format!("http://{addr}/v1/responses"))
            .bearer_auth("secret")
            .json(&json!({
                "input": "first",
                "metadata": {"session_id": "http-integration"}
            }))
            .send()
            .await
            .expect("send first request")
            .json::<Value>()
            .await
            .expect("parse first response");
        let first_count = response_one["output_text"]
            .as_str()
            .unwrap_or_default()
            .trim_start_matches("messages=")
            .parse::<usize>()
            .expect("parse first count");

        let response_two = client
            .post(format!("http://{addr}/v1/responses"))
            .bearer_auth("secret")
            .json(&json!({
                "input": "second",
                "metadata": {"session_id": "http-integration"}
            }))
            .send()
            .await
            .expect("send second request")
            .json::<Value>()
            .await
            .expect("parse second response");
        let second_count = response_two["output_text"]
            .as_str()
            .unwrap_or_default()
            .trim_start_matches("messages=")
            .parse::<usize>()
            .expect("parse second count");

        assert!(second_count > first_count);

        let session_path = gateway_session_path(
            &state.config.state_dir,
            &sanitize_session_key("http-integration"),
        );
        assert!(session_path.exists());
        let raw = std::fs::read_to_string(&session_path).expect("read session file");
        assert!(raw.lines().count() >= 4);

        handle.abort();
    }

    #[tokio::test]
    async fn integration_gateway_status_endpoint_returns_service_snapshot() {
        let temp = tempdir().expect("tempdir");
        let state = test_state(temp.path(), 10_000, "secret");
        let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

        let client = Client::new();
        let payload = client
            .get(format!("http://{addr}{GATEWAY_STATUS_ENDPOINT}"))
            .bearer_auth("secret")
            .send()
            .await
            .expect("send status request")
            .json::<Value>()
            .await
            .expect("parse status response");

        assert_eq!(
            payload["gateway"]["responses_endpoint"].as_str(),
            Some(OPENRESPONSES_ENDPOINT)
        );
        assert_eq!(
            payload["gateway"]["webchat_endpoint"].as_str(),
            Some(WEBCHAT_ENDPOINT)
        );
        assert_eq!(
            payload["gateway"]["status_endpoint"].as_str(),
            Some(GATEWAY_STATUS_ENDPOINT)
        );
        assert_eq!(
            payload["service"]["service_status"].as_str(),
            Some("running")
        );

        handle.abort();
    }

    #[tokio::test]
    async fn regression_openresponses_endpoint_rejects_malformed_json_body() {
        let temp = tempdir().expect("tempdir");
        let state = test_state(temp.path(), 10_000, "secret");
        let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

        let client = Client::new();
        let response = client
            .post(format!("http://{addr}/v1/responses"))
            .bearer_auth("secret")
            .header("content-type", "application/json")
            .body("{invalid")
            .send()
            .await
            .expect("send malformed request");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        handle.abort();
    }

    #[tokio::test]
    async fn regression_openresponses_endpoint_rejects_oversized_input() {
        let temp = tempdir().expect("tempdir");
        let state = test_state(temp.path(), 8, "secret");
        let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

        let client = Client::new();
        let response = client
            .post(format!("http://{addr}/v1/responses"))
            .bearer_auth("secret")
            .json(&json!({"input": "this request is too large"}))
            .send()
            .await
            .expect("send oversized request");

        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        handle.abort();
    }

    #[tokio::test]
    async fn regression_gateway_status_endpoint_rejects_unauthorized_request() {
        let temp = tempdir().expect("tempdir");
        let state = test_state(temp.path(), 10_000, "secret");
        let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

        let client = Client::new();
        let response = client
            .get(format!("http://{addr}{GATEWAY_STATUS_ENDPOINT}"))
            .send()
            .await
            .expect("send request");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        handle.abort();
    }

    #[test]
    fn regression_validate_gateway_openresponses_bind_rejects_invalid_socket_address() {
        let error = validate_gateway_openresponses_bind("invalid-bind")
            .expect_err("invalid bind should fail");
        assert!(error
            .to_string()
            .contains("invalid gateway socket address 'invalid-bind'"));
    }
}
