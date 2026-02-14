//! End-to-end HTTP provider tests for OpenAI-compatible client behavior.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use httpmock::{prelude::*, Mock};
use serde::Deserialize;
use serde_json::Value;
use tau_ai::{
    ChatRequest, LlmClient, Message, OpenAiAuthScheme, OpenAiClient, OpenAiConfig, TauAiError,
};

const OPENAI_HTTP_FIXTURE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize)]
struct OpenAiHttpFixture {
    schema_version: u32,
    name: String,
    prompt: String,
    steps: Vec<OpenAiHttpStep>,
    #[serde(default)]
    expected_text: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct OpenAiHttpStep {
    status: u16,
    body: Value,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    #[serde(default)]
    delay_ms: u64,
}

fn openai_http_fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join("openai-http")
        .join(name)
}

fn load_openai_http_fixture(name: &str) -> OpenAiHttpFixture {
    let path = openai_http_fixture_path(name);
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    let fixture = serde_json::from_str::<OpenAiHttpFixture>(&raw)
        .unwrap_or_else(|error| panic!("invalid fixture {}: {error}", path.display()));
    assert_eq!(
        fixture.schema_version,
        OPENAI_HTTP_FIXTURE_SCHEMA_VERSION,
        "unsupported schema_version in {}",
        path.display()
    );
    fixture
}

fn test_request(prompt: &str) -> ChatRequest {
    ChatRequest {
        model: "gpt-4o-mini".to_string(),
        messages: vec![Message::user(prompt)],
        tools: Vec::new(),
        tool_choice: None,
        json_mode: false,
        max_tokens: None,
        temperature: None,
    }
}

fn test_client(
    server: &MockServer,
    request_timeout_ms: u64,
    max_retries: usize,
    retry_budget_ms: u64,
) -> OpenAiClient {
    OpenAiClient::new(OpenAiConfig {
        api_base: format!("{}/v1", server.base_url()),
        api_key: "test-openai-key".to_string(),
        organization: None,
        request_timeout_ms,
        max_retries,
        retry_budget_ms,
        retry_jitter: false,
        auth_scheme: OpenAiAuthScheme::Bearer,
        api_version: None,
    })
    .expect("build OpenAI client")
}

fn install_fixture_script<'a>(
    server: &'a MockServer,
    fixture: &OpenAiHttpFixture,
) -> Vec<Mock<'a>> {
    fixture
        .steps
        .iter()
        .enumerate()
        .map(|(attempt, step)| {
            let step = step.clone();
            let attempt_header = attempt.to_string();
            server.mock(move |when, then| {
                when.method(POST)
                    .path("/v1/chat/completions")
                    .header("authorization", "Bearer test-openai-key")
                    .header("x-tau-retry-attempt", attempt_header.as_str())
                    .header_exists("x-tau-request-id");
                let mut then = then.status(step.status);
                for (name, value) in &step.headers {
                    then = then.header(name.as_str(), value.as_str());
                }
                if step.delay_ms > 0 {
                    then = then.delay(Duration::from_millis(step.delay_ms));
                }
                then.json_body(step.body.clone());
            })
        })
        .collect()
}

#[test]
fn unit_openai_http_fixture_schema_guard_accepts_v1() {
    let fixture = load_openai_http_fixture("happy-path.json");
    assert_eq!(fixture.schema_version, OPENAI_HTTP_FIXTURE_SCHEMA_VERSION);
    assert_eq!(fixture.name, "happy-path");
}

#[tokio::test]
async fn functional_openai_client_happy_path_roundtrip_from_fixture() {
    let fixture = load_openai_http_fixture("happy-path.json");
    let server = MockServer::start();
    let mocks = install_fixture_script(&server, &fixture);
    let client = test_client(&server, 2_000, 0, 0);

    let response = client
        .complete(test_request(&fixture.prompt))
        .await
        .expect("happy-path request should succeed");
    assert_eq!(
        response.message.text_content(),
        fixture.expected_text.expect("expected_text")
    );
    assert_eq!(response.finish_reason.as_deref(), Some("stop"));
    mocks[0].assert_calls(1);
}

#[tokio::test]
async fn integration_openai_client_retries_once_then_succeeds_with_backoff() {
    let fixture = load_openai_http_fixture("retry-500-then-success.json");
    assert_eq!(fixture.steps.len(), 2);
    let server = MockServer::start();
    let mocks = install_fixture_script(&server, &fixture);
    let client = test_client(&server, 2_000, 2, 3_000);

    let started = Instant::now();
    let response = client
        .complete(test_request(&fixture.prompt))
        .await
        .expect("retry path should eventually succeed");
    let elapsed = started.elapsed();

    assert_eq!(
        response.message.text_content(),
        fixture.expected_text.expect("expected_text")
    );
    assert!(
        elapsed >= Duration::from_millis(150),
        "expected retry backoff delay before second attempt, elapsed={elapsed:?}"
    );
    mocks[0].assert_calls(1);
    mocks[1].assert_calls(1);
}

#[tokio::test]
async fn regression_openai_client_timeout_error_surfaces_for_slow_provider() {
    let fixture = load_openai_http_fixture("timeout-happy-body.json");
    let server = MockServer::start();
    let mocks = install_fixture_script(&server, &fixture);
    let client = test_client(&server, 30, 0, 0);

    let error = client
        .complete(test_request(&fixture.prompt))
        .await
        .expect_err("slow provider should time out");
    match error {
        TauAiError::Http(inner) => assert!(inner.is_timeout(), "expected timeout error: {inner}"),
        other => panic!("expected TauAiError::Http timeout, got {other:?}"),
    }
    mocks[0].assert_calls(1);
}

#[tokio::test]
async fn regression_openai_client_malformed_success_payload_returns_invalid_response() {
    let fixture = load_openai_http_fixture("malformed-success-payload.json");
    let server = MockServer::start();
    let mocks = install_fixture_script(&server, &fixture);
    let client = test_client(&server, 2_000, 0, 0);

    let error = client
        .complete(test_request(&fixture.prompt))
        .await
        .expect_err("malformed success payload should fail parsing");
    match error {
        TauAiError::Serde(inner) => {
            assert!(inner.to_string().contains("missing field `choices`"));
        }
        other => panic!("expected TauAiError::Serde, got {other:?}"),
    }
    mocks[0].assert_calls(1);
}

#[tokio::test]
async fn regression_openai_client_retry_budget_blocks_followup_attempts_and_surfaces_5xx() {
    let fixture = load_openai_http_fixture("retry-500-then-success.json");
    let server = MockServer::start();
    let mocks = install_fixture_script(&server, &fixture);
    let client = test_client(&server, 2_000, 2, 10);

    let error = client
        .complete(test_request(&fixture.prompt))
        .await
        .expect_err("retry budget should block retry and surface first 5xx");
    match error {
        TauAiError::HttpStatus { status, body } => {
            assert_eq!(status, 500);
            assert!(body.contains("upstream temporary failure"));
        }
        other => panic!("expected TauAiError::HttpStatus(500), got {other:?}"),
    }
    mocks[0].assert_calls(1);
    mocks[1].assert_calls(0);
}
