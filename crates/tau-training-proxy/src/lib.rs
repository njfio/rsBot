//! OpenAI-compatible training attribution proxy runtime.

use anyhow::{bail, Context, Result};
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, HeaderName, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tau_core::current_unix_timestamp_ms;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

const PROXY_SCHEMA_VERSION: u32 = 1;
const TRAINING_PROXY_HEALTH_ENDPOINT: &str = "/training/proxy/health";
const OPENAI_CHAT_COMPLETIONS_ENDPOINT: &str = "/v1/chat/completions";
const ATTRIBUTION_LOG_FILE: &str = "proxy-attribution.jsonl";
const HEADER_ROLLOUT_ID: &str = "x-rollout-id";
const HEADER_ATTEMPT_ID: &str = "x-attempt-id";
const HEADER_SEQUENCE_ID: &str = "x-sequence-id";
const HEADER_TRACE_ID: &str = "x-trace-id";

#[derive(Debug, Clone)]
/// Public struct `TrainingProxyConfig` used across Tau components.
pub struct TrainingProxyConfig {
    pub bind: String,
    pub upstream_base_url: String,
    pub state_dir: PathBuf,
    pub request_timeout_ms: u64,
}

impl Default for TrainingProxyConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:8788".to_string(),
            upstream_base_url: String::new(),
            state_dir: PathBuf::from(".tau"),
            request_timeout_ms: 30_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `TrainingProxyAttribution` used across Tau components.
pub struct TrainingProxyAttribution {
    pub rollout_id: String,
    pub attempt_id: String,
    pub sequence_id: Option<u64>,
    pub trace_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct TrainingProxyAttributionRecord {
    schema_version: u32,
    timestamp_unix_ms: u64,
    method: String,
    path: String,
    upstream_url: String,
    rollout_id: String,
    attempt_id: String,
    sequence_id: Option<u64>,
    trace_id: Option<String>,
    request_bytes: usize,
    response_bytes: usize,
    duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    status_code: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_message: Option<String>,
}

#[derive(Clone)]
struct TrainingProxyState {
    client: Client,
    upstream_chat_completions_url: String,
    attribution_log_path: PathBuf,
}

impl TrainingProxyState {
    fn from_config(config: &TrainingProxyConfig) -> Result<Self> {
        let upstream_base_url = config.upstream_base_url.trim();
        if upstream_base_url.is_empty() {
            bail!("--training-proxy-upstream-url must be provided");
        }

        let request_timeout_ms = config.request_timeout_ms.max(1_000);
        let client = Client::builder()
            .timeout(Duration::from_millis(request_timeout_ms))
            .build()
            .context("failed to construct reqwest client for training proxy")?;

        let training_root = resolve_training_root(&config.state_dir);
        std::fs::create_dir_all(&training_root).with_context(|| {
            format!(
                "failed to create training proxy state directory '{}'",
                training_root.display()
            )
        })?;

        Ok(Self {
            client,
            upstream_chat_completions_url: format!(
                "{}/v1/chat/completions",
                upstream_base_url.trim_end_matches('/')
            ),
            attribution_log_path: training_root.join(ATTRIBUTION_LOG_FILE),
        })
    }
}

/// Run the OpenAI-compatible training attribution proxy server.
pub async fn run_training_proxy(config: TrainingProxyConfig) -> Result<()> {
    let bind_addr: SocketAddr = config.bind.parse().with_context(|| {
        format!(
            "invalid --training-proxy-bind '{}': expected host:port",
            config.bind
        )
    })?;
    let state = Arc::new(TrainingProxyState::from_config(&config)?);

    let listener = TcpListener::bind(bind_addr)
        .await
        .with_context(|| format!("failed to bind training proxy on {bind_addr}"))?;
    let local_addr = listener
        .local_addr()
        .context("failed to resolve training proxy listen address")?;

    println!(
        "training proxy listening: addr={} upstream={} attribution_log={}",
        local_addr,
        state.upstream_chat_completions_url,
        state.attribution_log_path.display()
    );

    let app = build_training_proxy_router(state);
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .context("training proxy server exited unexpectedly")?;
    Ok(())
}

/// Parse rollout attribution headers from proxy requests.
pub fn parse_training_proxy_attribution(headers: &HeaderMap) -> Result<TrainingProxyAttribution> {
    let rollout_id = parse_required_header(headers, HEADER_ROLLOUT_ID)?;
    let attempt_id = parse_required_header(headers, HEADER_ATTEMPT_ID)?;
    let sequence_id = parse_optional_u64_header(headers, HEADER_SEQUENCE_ID)?;
    let trace_id = parse_optional_header(headers, HEADER_TRACE_ID)?;

    Ok(TrainingProxyAttribution {
        rollout_id,
        attempt_id,
        sequence_id,
        trace_id,
    })
}

fn resolve_training_root(state_dir: &Path) -> PathBuf {
    state_dir.join("training")
}

fn parse_required_header(headers: &HeaderMap, name: &str) -> Result<String> {
    let Some(value) = headers.get(name) else {
        bail!("missing required header '{name}'");
    };
    let parsed = value
        .to_str()
        .with_context(|| format!("header '{name}' must be valid utf-8"))?
        .trim()
        .to_string();
    if parsed.is_empty() {
        bail!("header '{name}' cannot be empty");
    }
    Ok(parsed)
}

fn parse_optional_header(headers: &HeaderMap, name: &str) -> Result<Option<String>> {
    let Some(value) = headers.get(name) else {
        return Ok(None);
    };
    let parsed = value
        .to_str()
        .with_context(|| format!("header '{name}' must be valid utf-8"))?
        .trim()
        .to_string();
    if parsed.is_empty() {
        return Ok(None);
    }
    Ok(Some(parsed))
}

fn parse_optional_u64_header(headers: &HeaderMap, name: &str) -> Result<Option<u64>> {
    let Some(raw) = parse_optional_header(headers, name)? else {
        return Ok(None);
    };
    let parsed = raw
        .parse::<u64>()
        .with_context(|| format!("header '{name}' must be an unsigned integer"))?;
    Ok(Some(parsed))
}

fn build_training_proxy_router(state: Arc<TrainingProxyState>) -> Router {
    Router::new()
        .route(TRAINING_PROXY_HEALTH_ENDPOINT, get(handle_health))
        .route(
            OPENAI_CHAT_COMPLETIONS_ENDPOINT,
            post(handle_chat_completions),
        )
        .with_state(state)
}

async fn handle_health(State(state): State<Arc<TrainingProxyState>>) -> Response {
    (
        StatusCode::OK,
        Json(json!({
            "schema_version": PROXY_SCHEMA_VERSION,
            "status": "ready",
            "upstream_chat_completions_url": state.upstream_chat_completions_url,
            "attribution_log_path": state.attribution_log_path.display().to_string(),
        })),
    )
        .into_response()
}

async fn handle_chat_completions(
    State(state): State<Arc<TrainingProxyState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let attribution = match parse_training_proxy_attribution(&headers) {
        Ok(parsed) => parsed,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": {
                        "code": "training_proxy_missing_or_invalid_attribution_header",
                        "message": error.to_string(),
                    }
                })),
            )
                .into_response();
        }
    };

    let started_unix_ms = current_unix_timestamp_ms();
    let mut request = state
        .client
        .post(state.upstream_chat_completions_url.as_str())
        .body(body.to_vec());
    for (header_name, header_value) in &headers {
        if should_forward_header(header_name) {
            request = request.header(header_name, header_value);
        }
    }

    let upstream_response = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            let duration_ms = current_unix_timestamp_ms().saturating_sub(started_unix_ms);
            let record = TrainingProxyAttributionRecord {
                schema_version: PROXY_SCHEMA_VERSION,
                timestamp_unix_ms: current_unix_timestamp_ms(),
                method: "POST".to_string(),
                path: OPENAI_CHAT_COMPLETIONS_ENDPOINT.to_string(),
                upstream_url: state.upstream_chat_completions_url.clone(),
                rollout_id: attribution.rollout_id,
                attempt_id: attribution.attempt_id,
                sequence_id: attribution.sequence_id,
                trace_id: attribution.trace_id,
                request_bytes: body.len(),
                response_bytes: 0,
                duration_ms,
                status_code: None,
                error_code: Some("upstream_request_failed".to_string()),
                error_message: Some(error.to_string()),
            };
            let _ = append_attribution_record(&state.attribution_log_path, &record).await;
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({
                    "error": {
                        "code": "training_proxy_upstream_request_failed",
                        "message": error.to_string(),
                    }
                })),
            )
                .into_response();
        }
    };

    let response_status = upstream_response.status();
    let response_headers = upstream_response.headers().clone();
    let response_body = match upstream_response.bytes().await {
        Ok(bytes) => bytes,
        Err(error) => {
            let duration_ms = current_unix_timestamp_ms().saturating_sub(started_unix_ms);
            let record = TrainingProxyAttributionRecord {
                schema_version: PROXY_SCHEMA_VERSION,
                timestamp_unix_ms: current_unix_timestamp_ms(),
                method: "POST".to_string(),
                path: OPENAI_CHAT_COMPLETIONS_ENDPOINT.to_string(),
                upstream_url: state.upstream_chat_completions_url.clone(),
                rollout_id: attribution.rollout_id,
                attempt_id: attribution.attempt_id,
                sequence_id: attribution.sequence_id,
                trace_id: attribution.trace_id,
                request_bytes: body.len(),
                response_bytes: 0,
                duration_ms,
                status_code: Some(response_status.as_u16()),
                error_code: Some("upstream_response_read_failed".to_string()),
                error_message: Some(error.to_string()),
            };
            let _ = append_attribution_record(&state.attribution_log_path, &record).await;
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({
                    "error": {
                        "code": "training_proxy_upstream_response_read_failed",
                        "message": error.to_string(),
                    }
                })),
            )
                .into_response();
        }
    };

    let duration_ms = current_unix_timestamp_ms().saturating_sub(started_unix_ms);
    let record = TrainingProxyAttributionRecord {
        schema_version: PROXY_SCHEMA_VERSION,
        timestamp_unix_ms: current_unix_timestamp_ms(),
        method: "POST".to_string(),
        path: OPENAI_CHAT_COMPLETIONS_ENDPOINT.to_string(),
        upstream_url: state.upstream_chat_completions_url.clone(),
        rollout_id: attribution.rollout_id,
        attempt_id: attribution.attempt_id,
        sequence_id: attribution.sequence_id,
        trace_id: attribution.trace_id,
        request_bytes: body.len(),
        response_bytes: response_body.len(),
        duration_ms,
        status_code: Some(response_status.as_u16()),
        error_code: None,
        error_message: None,
    };
    let _ = append_attribution_record(&state.attribution_log_path, &record).await;

    let mut response = Response::new(Body::from(response_body.to_vec()));
    *response.status_mut() = response_status;
    if let Some(content_type) = response_headers.get(CONTENT_TYPE) {
        response
            .headers_mut()
            .insert(CONTENT_TYPE, content_type.clone());
    }
    response
}

fn should_forward_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "authorization"
            | "content-type"
            | "openai-organization"
            | "openai-project"
            | "openai-beta"
            | "x-request-id"
    )
}

async fn append_attribution_record(
    path: &Path,
    record: &TrainingProxyAttributionRecord,
) -> Result<()> {
    let payload =
        serde_json::to_vec(record).context("failed to encode training proxy attribution record")?;
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
        .with_context(|| format!("failed to open attribution log '{}'", path.display()))?;
    file.write_all(&payload).await.with_context(|| {
        format!(
            "failed to write attribution log payload to '{}'",
            path.display()
        )
    })?;
    file.write_all(b"\n").await.with_context(|| {
        format!(
            "failed to write attribution log newline to '{}'",
            path.display()
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::http::Request;
    use httpmock::Method::POST;
    use httpmock::MockServer;
    use serde_json::Value;
    use tempfile::tempdir;
    use tower::ServiceExt;

    #[test]
    fn unit_parse_training_proxy_attribution_reads_required_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(HEADER_ROLLOUT_ID, "rollout-1".parse().expect("header"));
        headers.insert(HEADER_ATTEMPT_ID, "attempt-2".parse().expect("header"));
        headers.insert(HEADER_SEQUENCE_ID, "7".parse().expect("header"));
        headers.insert(HEADER_TRACE_ID, "trace-abc".parse().expect("header"));

        let parsed = parse_training_proxy_attribution(&headers).expect("parse attribution");
        assert_eq!(parsed.rollout_id, "rollout-1");
        assert_eq!(parsed.attempt_id, "attempt-2");
        assert_eq!(parsed.sequence_id, Some(7));
        assert_eq!(parsed.trace_id.as_deref(), Some("trace-abc"));
    }

    #[test]
    fn regression_parse_training_proxy_attribution_rejects_missing_required_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(HEADER_ROLLOUT_ID, "rollout-1".parse().expect("header"));

        let error = parse_training_proxy_attribution(&headers).expect_err("missing attempt id");
        assert!(error
            .to_string()
            .contains("missing required header 'x-attempt-id'"));
    }

    #[tokio::test]
    async fn integration_proxy_forwards_request_and_persists_attribution_log() {
        let upstream = MockServer::start_async().await;
        let forwarded = upstream.mock(|when, then| {
            when.method(POST)
                .path("/v1/chat/completions")
                .header("authorization", "Bearer test-token");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"id":"chatcmpl-1","object":"chat.completion"}"#);
        });

        let temp = tempdir().expect("tempdir");
        let state = Arc::new(
            TrainingProxyState::from_config(&TrainingProxyConfig {
                bind: "127.0.0.1:0".to_string(),
                upstream_base_url: upstream.base_url(),
                state_dir: temp.path().join(".tau"),
                request_timeout_ms: 10_000,
            })
            .expect("build proxy state"),
        );
        let app = build_training_proxy_router(state.clone());

        let request = Request::builder()
            .method("POST")
            .uri(OPENAI_CHAT_COMPLETIONS_ENDPOINT)
            .header("authorization", "Bearer test-token")
            .header("content-type", "application/json")
            .header(HEADER_ROLLOUT_ID, "rollout-1")
            .header(HEADER_ATTEMPT_ID, "attempt-2")
            .header(HEADER_SEQUENCE_ID, "7")
            .body(Body::from(r#"{"model":"gpt-4o-mini","messages":[]}"#))
            .expect("request");

        let response = app.oneshot(request).await.expect("proxy response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read response body");
        let parsed: serde_json::Value =
            serde_json::from_slice(&body).expect("parse response body as json");
        assert_eq!(parsed["id"], "chatcmpl-1");

        forwarded.assert();
        let attribution_log =
            std::fs::read_to_string(&state.attribution_log_path).expect("read attribution log");
        assert!(attribution_log.contains("\"rollout_id\":\"rollout-1\""));
        assert!(attribution_log.contains("\"attempt_id\":\"attempt-2\""));
        assert!(attribution_log.contains("\"sequence_id\":7"));
        assert!(attribution_log.contains("\"status_code\":200"));
    }

    #[tokio::test]
    async fn regression_proxy_rejects_requests_without_required_attribution_headers() {
        let upstream = MockServer::start_async().await;
        let temp = tempdir().expect("tempdir");
        let state = Arc::new(
            TrainingProxyState::from_config(&TrainingProxyConfig {
                bind: "127.0.0.1:0".to_string(),
                upstream_base_url: upstream.base_url(),
                state_dir: temp.path().join(".tau"),
                request_timeout_ms: 10_000,
            })
            .expect("build proxy state"),
        );
        let app = build_training_proxy_router(state);

        let request = Request::builder()
            .method("POST")
            .uri(OPENAI_CHAT_COMPLETIONS_ENDPOINT)
            .header("content-type", "application/json")
            .body(Body::from(r#"{"model":"gpt-4o-mini","messages":[]}"#))
            .expect("request");

        let response = app.oneshot(request).await.expect("proxy response");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read response body");
        let parsed: serde_json::Value =
            serde_json::from_slice(&body).expect("parse error body as json");
        assert_eq!(
            parsed["error"]["code"],
            Value::String("training_proxy_missing_or_invalid_attribution_header".to_string())
        );
    }
}
