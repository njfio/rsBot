//! Gateway OpenResponses tests grouped by runtime behavior.
use super::*;
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use reqwest::Client;
use serde_json::Value;
use tau_ai::{ChatRequest, ChatResponse, ChatUsage, Message, TauAiError};
use tempfile::tempdir;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{self, client::IntoClientRequest, http::HeaderValue, Message as ClientWsMessage},
};

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

fn test_state_with_auth(
    root: &Path,
    max_input_chars: usize,
    auth_mode: GatewayOpenResponsesAuthMode,
    token: Option<&str>,
    password: Option<&str>,
    rate_limit_window_seconds: u64,
    rate_limit_max_requests: usize,
) -> Arc<GatewayOpenResponsesServerState> {
    Arc::new(GatewayOpenResponsesServerState::new(
        GatewayOpenResponsesServerConfig {
            client: Arc::new(MockGatewayLlmClient::default()),
            model: "openai/gpt-4o-mini".to_string(),
            system_prompt: "You are Tau.".to_string(),
            max_turns: 4,
            tool_registrar: Arc::new(NoopGatewayToolRegistrar),
            turn_timeout_ms: 0,
            session_lock_wait_ms: 500,
            session_lock_stale_ms: 10_000,
            state_dir: root.join(".tau/gateway"),
            bind: "127.0.0.1:0".to_string(),
            auth_mode,
            auth_token: token.map(str::to_string),
            auth_password: password.map(str::to_string),
            session_ttl_seconds: 3_600,
            rate_limit_window_seconds,
            rate_limit_max_requests,
            max_input_chars,
        },
    ))
}

fn test_state(
    root: &Path,
    max_input_chars: usize,
    token: &str,
) -> Arc<GatewayOpenResponsesServerState> {
    test_state_with_auth(
        root,
        max_input_chars,
        GatewayOpenResponsesAuthMode::Token,
        Some(token),
        None,
        60,
        120,
    )
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

fn write_multi_channel_runtime_fixture(root: &Path, with_connectors: bool) -> PathBuf {
    let multi_channel_root = root.join(".tau").join("multi-channel");
    std::fs::create_dir_all(&multi_channel_root).expect("create multi-channel root");
    std::fs::write(
        multi_channel_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_event_keys": ["telegram:tg-1", "discord:dc-1", "telegram:tg-2"],
  "health": {
    "updated_unix_ms": 981,
    "cycle_duration_ms": 14,
    "queue_depth": 2,
    "active_runs": 0,
    "failure_streak": 1,
    "last_cycle_discovered": 3,
    "last_cycle_processed": 3,
    "last_cycle_completed": 2,
    "last_cycle_failed": 1,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write multi-channel state");
    std::fs::write(
            multi_channel_root.join("runtime-events.jsonl"),
            r#"{"reason_codes":["events_applied","connector_retry"],"health_reason":"connector retry in progress"}
invalid-json-line
{"reason_codes":["connector_retry"],"health_reason":"connector retry in progress"}
"#,
        )
        .expect("write runtime events");
    if with_connectors {
        std::fs::write(
            multi_channel_root.join("live-connectors-state.json"),
            r#"{
  "schema_version": 1,
  "processed_event_keys": ["telegram:tg-1"],
  "channels": {
    "telegram": {
      "mode": "polling",
      "liveness": "open",
      "events_ingested": 6,
      "duplicates_skipped": 2,
      "retry_attempts": 3,
      "auth_failures": 1,
      "parse_failures": 0,
      "provider_failures": 2,
      "consecutive_failures": 2,
      "retry_budget_remaining": 0,
      "breaker_state": "open",
      "breaker_open_until_unix_ms": 4000,
      "breaker_last_open_reason": "provider_unavailable",
      "breaker_open_count": 1,
      "last_error_code": "provider_unavailable"
    }
  }
}
"#,
        )
        .expect("write connectors state");
    }
    multi_channel_root
}

async fn connect_gateway_ws(
    addr: SocketAddr,
    token: Option<&str>,
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
> {
    let uri = format!("ws://{addr}{GATEWAY_WS_ENDPOINT}");
    let mut request = uri
        .into_client_request()
        .context("failed to construct websocket request")?;
    if let Some(token) = token {
        request.headers_mut().insert(
            AUTHORIZATION,
            HeaderValue::from_str(format!("Bearer {token}").as_str())
                .expect("valid bearer auth header"),
        );
    }
    let (socket, _) = connect_async(request)
        .await
        .context("failed to establish websocket connection")?;
    Ok(socket)
}

async fn recv_gateway_ws_json(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Value {
    let message = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let Some(message) = socket.next().await else {
                panic!("websocket closed before response frame");
            };
            let message = message.expect("read websocket frame");
            match message {
                ClientWsMessage::Text(text) => {
                    return serde_json::from_str::<Value>(text.as_str())
                        .expect("websocket text frame should contain json");
                }
                ClientWsMessage::Ping(payload) => {
                    socket
                        .send(ClientWsMessage::Pong(payload))
                        .await
                        .expect("send pong");
                }
                ClientWsMessage::Pong(_) => continue,
                ClientWsMessage::Binary(_) => continue,
                ClientWsMessage::Close(_) => panic!("websocket closed before json frame"),
                ClientWsMessage::Frame(_) => continue,
            }
        }
    })
    .await
    .expect("websocket response should arrive before timeout");
    message
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

    let translated = translate_openresponses_request(&request, 10_000).expect("translate request");
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
fn unit_collect_gateway_multi_channel_status_report_composes_runtime_and_connector_fields() {
    let temp = tempdir().expect("tempdir");
    let gateway_state_dir = temp.path().join(".tau").join("gateway");
    std::fs::create_dir_all(&gateway_state_dir).expect("create gateway state dir");
    write_multi_channel_runtime_fixture(temp.path(), true);

    let report = collect_gateway_multi_channel_status_report(&gateway_state_dir);
    assert!(report.state_present);
    assert_eq!(report.health_state, "degraded");
    assert_eq!(report.rollout_gate, "hold");
    assert_eq!(report.health_reason, "connector retry in progress");
    assert_eq!(report.processed_event_count, 3);
    assert_eq!(report.transport_counts.get("telegram"), Some(&2));
    assert_eq!(report.transport_counts.get("discord"), Some(&1));
    assert_eq!(report.queue_depth, 2);
    assert_eq!(report.failure_streak, 1);
    assert_eq!(report.cycle_reports, 2);
    assert_eq!(report.invalid_cycle_reports, 1);
    assert_eq!(report.reason_code_counts.get("events_applied"), Some(&1));
    assert_eq!(report.reason_code_counts.get("connector_retry"), Some(&2));
    assert!(report.connectors.state_present);
    assert_eq!(report.connectors.processed_event_count, 1);
    let telegram = report
        .connectors
        .channels
        .get("telegram")
        .expect("telegram connector");
    assert_eq!(telegram.liveness, "open");
    assert_eq!(telegram.breaker_state, "open");
    assert_eq!(telegram.provider_failures, 2);
}

#[test]
fn unit_render_gateway_webchat_page_includes_expected_endpoints() {
    let html = render_gateway_webchat_page();
    assert!(html.contains("Tau Gateway Webchat"));
    assert!(html.contains(OPENRESPONSES_ENDPOINT));
    assert!(html.contains(GATEWAY_STATUS_ENDPOINT));
    assert!(html.contains(GATEWAY_WS_ENDPOINT));
    assert!(html.contains(DEFAULT_SESSION_KEY));
    assert!(html.contains("multi_channel_lifecycle: state_present="));
    assert!(html.contains("connector_channels:"));
}

#[test]
fn unit_render_gateway_dashboard_page_includes_expected_endpoints() {
    let html = render_gateway_dashboard_page();
    assert!(html.contains("Tau Operator Dashboard"));
    assert!(html.contains(OPENRESPONSES_ENDPOINT));
    assert!(html.contains(DASHBOARD_ENDPOINT));
    assert!(html.contains(WEBCHAT_ENDPOINT));
    assert!(html.contains(GATEWAY_STATUS_ENDPOINT));
    assert!(html.contains(GATEWAY_WS_ENDPOINT));
    assert!(html.contains(DEFAULT_SESSION_KEY));
    assert!(html.contains("Operator Controls"));
    assert!(html.contains("Transport Table"));
    assert!(html.contains("Reason Codes"));
}

#[tokio::test]
async fn functional_dashboard_endpoint_returns_html_shell() {
    let temp = tempdir().expect("tempdir");
    let state = test_state(temp.path(), 10_000, "secret");
    let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

    let client = Client::new();
    let response = client
        .get(format!("http://{addr}{DASHBOARD_ENDPOINT}"))
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
    let body = response.text().await.expect("read dashboard body");
    assert!(body.contains("Tau Operator Dashboard"));
    assert!(body.contains(OPENRESPONSES_ENDPOINT));
    assert!(body.contains(WEBCHAT_ENDPOINT));
    assert!(body.contains(GATEWAY_STATUS_ENDPOINT));
    assert!(body.contains(GATEWAY_WS_ENDPOINT));
    assert!(body.contains("Operator Controls"));
    assert!(body.contains("Transport Table"));
    assert!(body.contains("Reason Codes"));

    handle.abort();
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
    assert!(body.contains(GATEWAY_WS_ENDPOINT));
    assert!(body.contains("multi-channel lifecycle summary"));
    assert!(body.contains("connector counters"));
    assert!(body.contains("recent reason codes"));

    handle.abort();
}

#[tokio::test]
async fn regression_webchat_endpoint_remains_available_after_unauthorized_request() {
    let temp = tempdir().expect("tempdir");
    let state = test_state(temp.path(), 10_000, "secret");
    let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

    let client = Client::new();
    let unauthorized = client
        .post(format!("http://{addr}/v1/responses"))
        .json(&json!({"input":"hello"}))
        .send()
        .await
        .expect("send unauthorized request");
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let webchat = client
        .get(format!("http://{addr}{WEBCHAT_ENDPOINT}"))
        .send()
        .await
        .expect("send webchat request");
    assert_eq!(webchat.status(), StatusCode::OK);
    let body = webchat.text().await.expect("read webchat body");
    assert!(body.contains("Tau Gateway Webchat"));
    assert!(body.contains("Refresh status"));
    assert!(body.contains("clearOutput"));

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
async fn functional_gateway_auth_session_endpoint_issues_bearer_for_password_mode() {
    let temp = tempdir().expect("tempdir");
    let state = test_state_with_auth(
        temp.path(),
        10_000,
        GatewayOpenResponsesAuthMode::PasswordSession,
        None,
        Some("pw-secret"),
        60,
        120,
    );
    let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

    let client = Client::new();
    let issue_response = client
        .post(format!("http://{addr}{GATEWAY_AUTH_SESSION_ENDPOINT}"))
        .json(&json!({"password":"pw-secret"}))
        .send()
        .await
        .expect("send session issue request");
    assert_eq!(issue_response.status(), StatusCode::OK);
    let issue_payload = issue_response
        .json::<Value>()
        .await
        .expect("parse session payload");
    let session_token = issue_payload["access_token"]
        .as_str()
        .expect("access token present")
        .to_string();
    assert!(session_token.starts_with("tau_sess_"));

    let status_response = client
        .get(format!("http://{addr}{GATEWAY_STATUS_ENDPOINT}"))
        .bearer_auth(session_token)
        .send()
        .await
        .expect("send status request");
    assert_eq!(status_response.status(), StatusCode::OK);

    handle.abort();
}

#[tokio::test]
async fn functional_gateway_ws_endpoint_rejects_unauthorized_upgrade() {
    let temp = tempdir().expect("tempdir");
    let state = test_state(temp.path(), 10_000, "secret");
    let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

    let error = connect_async(format!("ws://{addr}{GATEWAY_WS_ENDPOINT}"))
        .await
        .expect_err("websocket upgrade should reject missing auth");
    match error {
        tungstenite::Error::Http(response) => {
            assert_eq!(
                response.status().as_u16(),
                StatusCode::UNAUTHORIZED.as_u16()
            );
        }
        other => panic!("expected HTTP upgrade rejection, got {other:?}"),
    }

    handle.abort();
}

#[tokio::test]
async fn functional_gateway_ws_endpoint_supports_capabilities_and_ping_pong() {
    let temp = tempdir().expect("tempdir");
    let state = test_state(temp.path(), 10_000, "secret");
    let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

    let mut socket = connect_gateway_ws(addr, Some("secret"))
        .await
        .expect("connect websocket");
    socket
            .send(ClientWsMessage::Text(
                r#"{"schema_version":1,"request_id":"req-cap","kind":"capabilities.request","payload":{}}"#
                    .into(),
            ))
            .await
            .expect("send capabilities frame");

    let response = recv_gateway_ws_json(&mut socket).await;
    assert_eq!(response["schema_version"], 1);
    assert_eq!(response["request_id"], "req-cap");
    assert_eq!(response["kind"], "capabilities.response");
    assert_eq!(response["payload"]["protocol_version"], "0.1.0");

    socket
        .send(ClientWsMessage::Ping(vec![7, 3, 1]))
        .await
        .expect("send ping");
    let pong = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let Some(message) = socket.next().await else {
                panic!("websocket closed before pong");
            };
            let message = message.expect("read websocket frame");
            if let ClientWsMessage::Pong(payload) = message {
                return payload;
            }
        }
    })
    .await
    .expect("pong should arrive before timeout");
    assert_eq!(pong.to_vec(), vec![7, 3, 1]);

    socket.close(None).await.expect("close websocket");
    handle.abort();
}

#[tokio::test]
async fn integration_gateway_ws_session_status_and_reset_roundtrip() {
    let temp = tempdir().expect("tempdir");
    let state = test_state(temp.path(), 10_000, "secret");
    let session_path = gateway_session_path(&state.config.state_dir, DEFAULT_SESSION_KEY);
    if let Some(parent) = session_path.parent() {
        std::fs::create_dir_all(parent).expect("create session parent");
    }
    std::fs::write(
        &session_path,
        r#"{"id":"seed-1","role":"system","content":"seed"}"#,
    )
    .expect("seed session file");

    let (addr, handle) = spawn_test_server(state).await.expect("spawn server");
    let mut socket = connect_gateway_ws(addr, Some("secret"))
        .await
        .expect("connect websocket");

    socket
            .send(ClientWsMessage::Text(
                r#"{"schema_version":1,"request_id":"req-status-before","kind":"session.status.request","payload":{}}"#
                    .into(),
            ))
            .await
            .expect("send status before");
    let before = recv_gateway_ws_json(&mut socket).await;
    assert_eq!(before["kind"], "session.status.response");
    assert_eq!(before["payload"]["session_key"], DEFAULT_SESSION_KEY);
    assert_eq!(before["payload"]["exists"], true);
    assert_eq!(before["payload"]["message_count"], 1);
    assert!(session_path.exists());

    socket
            .send(ClientWsMessage::Text(
                r#"{"schema_version":1,"request_id":"req-reset","kind":"session.reset.request","payload":{}}"#
                    .into(),
            ))
            .await
            .expect("send session reset");
    let reset = recv_gateway_ws_json(&mut socket).await;
    assert_eq!(reset["kind"], "session.reset.response");
    assert_eq!(reset["payload"]["session_key"], DEFAULT_SESSION_KEY);
    assert_eq!(reset["payload"]["reset"], true);
    assert!(!session_path.exists());

    socket
            .send(ClientWsMessage::Text(
                r#"{"schema_version":1,"request_id":"req-status-after","kind":"session.status.request","payload":{}}"#
                    .into(),
            ))
            .await
            .expect("send status after");
    let after = recv_gateway_ws_json(&mut socket).await;
    assert_eq!(after["kind"], "session.status.response");
    assert_eq!(after["payload"]["exists"], false);
    assert_eq!(after["payload"]["message_count"], 0);

    socket.close(None).await.expect("close websocket");
    handle.abort();
}

#[tokio::test]
async fn regression_gateway_ws_malformed_frame_fails_closed_without_crashing_runtime() {
    let temp = tempdir().expect("tempdir");
    let state = test_state(temp.path(), 10_000, "secret");
    let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

    let mut socket = connect_gateway_ws(addr, Some("secret"))
        .await
        .expect("connect websocket");
    socket
        .send(ClientWsMessage::Text("not-json".into()))
        .await
        .expect("send malformed frame");
    let malformed = recv_gateway_ws_json(&mut socket).await;
    assert_eq!(malformed["kind"], "error");
    assert_eq!(malformed["payload"]["code"], "invalid_json");

    socket
            .send(ClientWsMessage::Text(
                r#"{"schema_version":1,"request_id":"req-status","kind":"gateway.status.request","payload":{}}"#
                    .into(),
            ))
            .await
            .expect("send valid status frame");
    let status = recv_gateway_ws_json(&mut socket).await;
    assert_eq!(status["kind"], "gateway.status.response");
    assert_eq!(
        status["payload"]["gateway"]["dashboard_endpoint"],
        DASHBOARD_ENDPOINT
    );
    assert_eq!(
        status["payload"]["gateway"]["ws_endpoint"],
        GATEWAY_WS_ENDPOINT
    );
    assert_eq!(
        status["payload"]["multi_channel"]["state_present"],
        Value::Bool(false)
    );

    socket.close(None).await.expect("close websocket");
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
        payload["gateway"]["dashboard_endpoint"].as_str(),
        Some(DASHBOARD_ENDPOINT)
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
        payload["gateway"]["ws_endpoint"].as_str(),
        Some(GATEWAY_WS_ENDPOINT)
    );
    assert_eq!(
        payload["service"]["service_status"].as_str(),
        Some("running")
    );
    assert_eq!(
        payload["multi_channel"]["state_present"],
        Value::Bool(false)
    );
    assert_eq!(
        payload["multi_channel"]["health_state"],
        Value::String("unknown".to_string())
    );
    assert_eq!(
        payload["multi_channel"]["rollout_gate"],
        Value::String("hold".to_string())
    );
    assert_eq!(
        payload["multi_channel"]["connectors"]["state_present"],
        Value::Bool(false)
    );
    assert_eq!(
        payload["multi_channel"]["processed_event_count"],
        Value::Number(serde_json::Number::from(0))
    );

    handle.abort();
}

#[tokio::test]
async fn integration_gateway_status_endpoint_returns_expanded_multi_channel_health_payload() {
    let temp = tempdir().expect("tempdir");
    write_multi_channel_runtime_fixture(temp.path(), true);
    let state = test_state(temp.path(), 10_000, "secret");
    let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

    let payload = Client::new()
        .get(format!("http://{addr}{GATEWAY_STATUS_ENDPOINT}"))
        .bearer_auth("secret")
        .send()
        .await
        .expect("send status request")
        .json::<Value>()
        .await
        .expect("parse status response");

    assert_eq!(payload["multi_channel"]["state_present"], Value::Bool(true));
    assert_eq!(
        payload["multi_channel"]["health_state"],
        Value::String("degraded".to_string())
    );
    assert_eq!(
        payload["multi_channel"]["health_reason"],
        Value::String("connector retry in progress".to_string())
    );
    assert_eq!(
        payload["multi_channel"]["processed_event_count"],
        Value::Number(serde_json::Number::from(3))
    );
    assert_eq!(
        payload["multi_channel"]["transport_counts"]["telegram"],
        Value::Number(serde_json::Number::from(2))
    );
    assert_eq!(
        payload["multi_channel"]["transport_counts"]["discord"],
        Value::Number(serde_json::Number::from(1))
    );
    assert_eq!(
        payload["multi_channel"]["reason_code_counts"]["connector_retry"],
        Value::Number(serde_json::Number::from(2))
    );
    assert_eq!(
        payload["multi_channel"]["connectors"]["state_present"],
        Value::Bool(true)
    );
    assert_eq!(
        payload["multi_channel"]["connectors"]["channels"]["telegram"]["breaker_state"],
        Value::String("open".to_string())
    );
    assert_eq!(
        payload["multi_channel"]["connectors"]["channels"]["telegram"]["provider_failures"],
        Value::Number(serde_json::Number::from(2))
    );

    handle.abort();
}

#[test]
fn regression_collect_gateway_multi_channel_status_report_defaults_when_state_is_missing() {
    let temp = tempdir().expect("tempdir");
    let gateway_state_dir = temp.path().join(".tau").join("gateway");
    std::fs::create_dir_all(&gateway_state_dir).expect("create gateway state dir");

    let report = collect_gateway_multi_channel_status_report(&gateway_state_dir);
    assert!(!report.state_present);
    assert_eq!(report.health_state, "unknown");
    assert_eq!(report.rollout_gate, "hold");
    assert_eq!(report.processed_event_count, 0);
    assert!(report.connectors.channels.is_empty());
    assert!(!report.connectors.state_present);
    assert!(report
        .diagnostics
        .iter()
        .any(|line| line.starts_with("state_missing:")));
}

#[tokio::test]
async fn integration_localhost_dev_mode_allows_requests_without_bearer_token() {
    let temp = tempdir().expect("tempdir");
    let state = test_state_with_auth(
        temp.path(),
        10_000,
        GatewayOpenResponsesAuthMode::LocalhostDev,
        None,
        None,
        60,
        120,
    );
    let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

    let client = Client::new();
    let response = client
        .post(format!("http://{addr}{OPENRESPONSES_ENDPOINT}"))
        .json(&json!({"input":"hello localhost mode"}))
        .send()
        .await
        .expect("send request");
    assert_eq!(response.status(), StatusCode::OK);

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
async fn regression_gateway_auth_session_rejects_invalid_password() {
    let temp = tempdir().expect("tempdir");
    let state = test_state_with_auth(
        temp.path(),
        10_000,
        GatewayOpenResponsesAuthMode::PasswordSession,
        None,
        Some("pw-secret"),
        60,
        120,
    );
    let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

    let client = Client::new();
    let response = client
        .post(format!("http://{addr}{GATEWAY_AUTH_SESSION_ENDPOINT}"))
        .json(&json!({"password":"wrong"}))
        .send()
        .await
        .expect("send request");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let payload = response.json::<Value>().await.expect("parse response");
    assert_eq!(
        payload["error"]["code"].as_str(),
        Some("invalid_credentials")
    );

    handle.abort();
}

#[tokio::test]
async fn regression_gateway_password_session_token_expires_and_fails_closed() {
    let temp = tempdir().expect("tempdir");
    let state = Arc::new(GatewayOpenResponsesServerState::new(
        GatewayOpenResponsesServerConfig {
            client: Arc::new(MockGatewayLlmClient::default()),
            model: "openai/gpt-4o-mini".to_string(),
            system_prompt: "You are Tau.".to_string(),
            max_turns: 4,
            tool_registrar: Arc::new(NoopGatewayToolRegistrar),
            turn_timeout_ms: 0,
            session_lock_wait_ms: 500,
            session_lock_stale_ms: 10_000,
            state_dir: temp.path().join(".tau/gateway"),
            bind: "127.0.0.1:0".to_string(),
            auth_mode: GatewayOpenResponsesAuthMode::PasswordSession,
            auth_token: None,
            auth_password: Some("pw-secret".to_string()),
            session_ttl_seconds: 1,
            rate_limit_window_seconds: 60,
            rate_limit_max_requests: 120,
            max_input_chars: 10_000,
        },
    ));
    let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

    let client = Client::new();
    let issue_response = client
        .post(format!("http://{addr}{GATEWAY_AUTH_SESSION_ENDPOINT}"))
        .json(&json!({"password":"pw-secret"}))
        .send()
        .await
        .expect("issue session token");
    assert_eq!(issue_response.status(), StatusCode::OK);
    let token = issue_response
        .json::<Value>()
        .await
        .expect("parse issue response")["access_token"]
        .as_str()
        .expect("access token")
        .to_string();

    tokio::time::sleep(Duration::from_millis(1_100)).await;

    let status_response = client
        .get(format!("http://{addr}{GATEWAY_STATUS_ENDPOINT}"))
        .bearer_auth(token)
        .send()
        .await
        .expect("send status request");
    assert_eq!(status_response.status(), StatusCode::UNAUTHORIZED);

    handle.abort();
}

#[tokio::test]
async fn regression_gateway_rate_limit_rejects_excess_requests() {
    let temp = tempdir().expect("tempdir");
    let state = test_state_with_auth(
        temp.path(),
        10_000,
        GatewayOpenResponsesAuthMode::Token,
        Some("secret"),
        None,
        60,
        1,
    );
    let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

    let client = Client::new();
    let first = client
        .get(format!("http://{addr}{GATEWAY_STATUS_ENDPOINT}"))
        .bearer_auth("secret")
        .send()
        .await
        .expect("first request");
    assert_eq!(first.status(), StatusCode::OK);

    let second = client
        .get(format!("http://{addr}{GATEWAY_STATUS_ENDPOINT}"))
        .bearer_auth("secret")
        .send()
        .await
        .expect("second request");
    assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);

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
    let error =
        validate_gateway_openresponses_bind("invalid-bind").expect_err("invalid bind should fail");
    assert!(error
        .to_string()
        .contains("invalid gateway socket address 'invalid-bind'"));
}
