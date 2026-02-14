use httpmock::prelude::*;
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tau_ai::{
    AnthropicClient, AnthropicConfig, ChatRequest, GoogleClient, GoogleConfig, LlmClient, Message,
    OpenAiAuthScheme, OpenAiClient, OpenAiConfig, TauAiError, ToolChoice, ToolDefinition,
};

#[tokio::test]
async fn openai_client_sends_expected_http_request() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .header_exists("x-tau-request-id")
            .header("x-tau-retry-attempt", "0")
            .json_body_includes(
                json!({
                    "model": "gpt-4o-mini",
                    "messages": [{"role": "system"}, {"role": "user"}],
                    "tools": [{"type": "function"}],
                    "tool_choice": "auto",
                    "response_format": {"type": "json_object"}
                })
                .to_string(),
            );

        then.status(200).json_body(json!({
            "choices": [{
                "message": {
                    "content": "openai ok"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 5,
                "completion_tokens": 3,
                "total_tokens": 8
            }
        }));
    });

    let client = OpenAiClient::new(OpenAiConfig {
        api_base: format!("{}/v1", server.base_url()),
        api_key: "test-openai-key".to_string(),
        organization: None,
        request_timeout_ms: 5_000,
        max_retries: 2,
        retry_budget_ms: 0,
        retry_jitter: false,
        auth_scheme: OpenAiAuthScheme::Bearer,
        api_version: None,
    })
    .expect("openai client should be created");

    let request = ChatRequest {
        model: "gpt-4o-mini".to_string(),
        messages: vec![Message::system("system"), Message::user("hello")],
        tools: vec![ToolDefinition {
            name: "read".to_string(),
            description: "Read a file".to_string(),
            parameters: json!({"type":"object"}),
        }],
        tool_choice: Some(ToolChoice::Auto),
        json_mode: true,
        max_tokens: Some(128),
        temperature: Some(0.0),
    };

    let response = client
        .complete(request)
        .await
        .expect("openai completion should succeed");

    mock.assert();
    assert_eq!(response.message.text_content(), "openai ok");
    assert_eq!(response.usage.total_tokens, 8);
}

#[tokio::test]
async fn integration_openai_client_supports_azure_api_key_header_and_api_version_query() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/openai/deployments/test-deployment/chat/completions")
            .query_param("api-version", "2024-10-21")
            .header("api-key", "test-azure-key")
            .header_exists("x-tau-request-id")
            .header("x-tau-retry-attempt", "0")
            .json_body_includes(
                json!({
                    "model": "gpt-4o-mini",
                    "messages": [{"role": "user"}]
                })
                .to_string(),
            );
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "azure ok"},
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 4,
                "completion_tokens": 3,
                "total_tokens": 7
            }
        }));
    });

    let client = OpenAiClient::new(OpenAiConfig {
        api_base: format!("{}/openai/deployments/test-deployment", server.base_url()),
        api_key: "test-azure-key".to_string(),
        organization: None,
        request_timeout_ms: 5_000,
        max_retries: 2,
        retry_budget_ms: 0,
        retry_jitter: false,
        auth_scheme: OpenAiAuthScheme::ApiKeyHeader,
        api_version: Some("2024-10-21".to_string()),
    })
    .expect("openai client should be created");

    let response = client
        .complete(ChatRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![Message::user("hello")],
            tools: vec![],
            tool_choice: None,
            json_mode: false,
            max_tokens: None,
            temperature: None,
        })
        .await
        .expect("azure-compatible completion should succeed");

    mock.assert_calls(1);
    assert_eq!(response.message.text_content(), "azure ok");
    assert_eq!(response.usage.total_tokens, 7);
}

#[tokio::test]
async fn anthropic_client_sends_expected_http_request() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/messages")
            .header("x-api-key", "test-anthropic-key")
            .header("anthropic-version", "2023-06-01")
            .header_exists("x-tau-request-id")
            .header("x-tau-retry-attempt", "0")
            .json_body_includes(
                json!({
                    "model": "claude-sonnet-4-20250514",
                    "system": "system",
                    "messages": [{"role": "user"}],
                    "tools": [{"name": "read"}],
                    "tool_choice": {"type": "auto"}
                })
                .to_string(),
            );

        then.status(200).json_body(json!({
            "content": [
                {"type":"text","text":"thinking"},
                {"type":"tool_use","id":"toolu_1","name":"read","input":{"path":"README.md"}}
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 10, "output_tokens": 4}
        }));
    });

    let client = AnthropicClient::new(AnthropicConfig {
        api_base: format!("{}/v1", server.base_url()),
        api_key: "test-anthropic-key".to_string(),
        request_timeout_ms: 5_000,
        max_retries: 2,
        retry_budget_ms: 0,
        retry_jitter: false,
    })
    .expect("anthropic client should be created");

    let request = ChatRequest {
        model: "claude-sonnet-4-20250514".to_string(),
        messages: vec![Message::system("system"), Message::user("hello")],
        tools: vec![ToolDefinition {
            name: "read".to_string(),
            description: "Read a file".to_string(),
            parameters: json!({"type":"object"}),
        }],
        tool_choice: Some(ToolChoice::Auto),
        json_mode: false,
        max_tokens: Some(128),
        temperature: Some(0.0),
    };

    let response = client
        .complete(request)
        .await
        .expect("anthropic completion should succeed");

    mock.assert();
    assert_eq!(response.message.tool_calls().len(), 1);
    assert_eq!(response.finish_reason.as_deref(), Some("tool_use"));
    assert_eq!(response.usage.total_tokens, 14);
}

#[tokio::test]
async fn google_client_sends_expected_http_request() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/models/gemini-2.5-pro:generateContent")
            .query_param("key", "test-google-key")
            .header_exists("x-tau-request-id")
            .header("x-tau-retry-attempt", "0")
            .json_body_includes(
                json!({
                    "contents": [{"role": "user"}],
                    "tools": [{"functionDeclarations": [{"name": "read"}]}],
                    "toolConfig": {"functionCallingConfig": {"mode": "AUTO"}},
                    "generationConfig": {"responseMimeType": "application/json"}
                })
                .to_string(),
            );

        then.status(200).json_body(json!({
            "candidates": [{
                "content": {
                    "parts": [
                        {"text": "google ok"},
                        {"functionCall": {"name": "read", "args": {"path": "README.md"}}}
                    ]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 9,
                "candidatesTokenCount": 5,
                "totalTokenCount": 14
            }
        }));
    });

    let client = GoogleClient::new(GoogleConfig {
        api_base: server.base_url(),
        api_key: "test-google-key".to_string(),
        request_timeout_ms: 5_000,
        max_retries: 2,
        retry_budget_ms: 0,
        retry_jitter: false,
    })
    .expect("google client should be created");

    let request = ChatRequest {
        model: "gemini-2.5-pro".to_string(),
        messages: vec![Message::system("system"), Message::user("hello")],
        tools: vec![ToolDefinition {
            name: "read".to_string(),
            description: "Read a file".to_string(),
            parameters: json!({"type":"object"}),
        }],
        tool_choice: Some(ToolChoice::Auto),
        json_mode: true,
        max_tokens: Some(128),
        temperature: Some(0.0),
    };

    let response = client
        .complete(request)
        .await
        .expect("google completion should succeed");

    mock.assert();
    assert_eq!(response.message.text_content(), "google ok");
    assert_eq!(response.message.tool_calls().len(), 1);
    assert_eq!(response.usage.total_tokens, 14);
}

#[tokio::test]
async fn openai_client_surfaces_http_status_error() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST).path("/v1/chat/completions");
        then.status(401).body("unauthorized");
    });

    let client = OpenAiClient::new(OpenAiConfig {
        api_base: format!("{}/v1", server.base_url()),
        api_key: "test-openai-key".to_string(),
        organization: None,
        request_timeout_ms: 5_000,
        max_retries: 2,
        retry_budget_ms: 0,
        retry_jitter: false,
        auth_scheme: OpenAiAuthScheme::Bearer,
        api_version: None,
    })
    .expect("openai client should be created");

    let request = ChatRequest {
        model: "gpt-4o-mini".to_string(),
        messages: vec![Message::user("hello")],
        tools: vec![],
        tool_choice: None,
        json_mode: false,
        max_tokens: None,
        temperature: None,
    };

    let error = client
        .complete(request)
        .await
        .expect_err("request should fail with 401");

    match error {
        TauAiError::HttpStatus { status, body } => {
            assert_eq!(status, 401);
            assert!(body.contains("unauthorized"));
        }
        other => panic!("expected TauAiError::HttpStatus, got {other:?}"),
    }
}

#[tokio::test]
async fn openai_client_retries_on_rate_limit_then_succeeds() {
    let server = MockServer::start();
    let first = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("x-tau-retry-attempt", "0");
        then.status(429).body("rate limited");
    });
    let second = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("x-tau-retry-attempt", "1");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "ok after retry"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
        }));
    });

    let client = OpenAiClient::new(OpenAiConfig {
        api_base: format!("{}/v1", server.base_url()),
        api_key: "test-openai-key".to_string(),
        organization: None,
        request_timeout_ms: 5_000,
        max_retries: 2,
        retry_budget_ms: 0,
        retry_jitter: false,
        auth_scheme: OpenAiAuthScheme::Bearer,
        api_version: None,
    })
    .expect("openai client should be created");

    let response = client
        .complete(ChatRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![Message::user("hello")],
            tools: vec![],
            tool_choice: None,
            json_mode: false,
            max_tokens: None,
            temperature: None,
        })
        .await
        .expect("retry should eventually succeed");

    assert_eq!(response.message.text_content(), "ok after retry");
    first.assert_calls(1);
    second.assert_calls(1);
}

#[tokio::test]
async fn integration_openai_client_respects_retry_after_header_floor() {
    let server = MockServer::start();
    let first = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("x-tau-retry-attempt", "0");
        then.status(429)
            .header("retry-after", "1")
            .body("rate limited");
    });
    let second = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("x-tau-retry-attempt", "1");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "ok after retry-after"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
        }));
    });

    let client = OpenAiClient::new(OpenAiConfig {
        api_base: format!("{}/v1", server.base_url()),
        api_key: "test-openai-key".to_string(),
        organization: None,
        request_timeout_ms: 5_000,
        max_retries: 1,
        retry_budget_ms: 0,
        retry_jitter: false,
        auth_scheme: OpenAiAuthScheme::Bearer,
        api_version: None,
    })
    .expect("openai client should be created");

    let started = Instant::now();
    let response = client
        .complete(ChatRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![Message::user("hello")],
            tools: vec![],
            tool_choice: None,
            json_mode: false,
            max_tokens: None,
            temperature: None,
        })
        .await
        .expect("retry should eventually succeed");
    let elapsed_ms = started.elapsed().as_millis() as u64;

    assert_eq!(response.message.text_content(), "ok after retry-after");
    assert!(
        elapsed_ms >= 900,
        "Retry-After floor should dominate base backoff; elapsed={elapsed_ms}ms"
    );
    first.assert_calls(1);
    second.assert_calls(1);
}

#[tokio::test]
async fn openai_client_retry_budget_can_block_retries() {
    let server = MockServer::start();
    let first = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("x-tau-retry-attempt", "0");
        then.status(429).body("rate limited");
    });
    let second = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("x-tau-retry-attempt", "1");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "should not be reached"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
        }));
    });

    let client = OpenAiClient::new(OpenAiConfig {
        api_base: format!("{}/v1", server.base_url()),
        api_key: "test-openai-key".to_string(),
        organization: None,
        request_timeout_ms: 5_000,
        max_retries: 2,
        retry_budget_ms: 10,
        retry_jitter: true,
        auth_scheme: OpenAiAuthScheme::Bearer,
        api_version: None,
    })
    .expect("openai client should be created");

    let error = client
        .complete(ChatRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![Message::user("hello")],
            tools: vec![],
            tool_choice: None,
            json_mode: false,
            max_tokens: None,
            temperature: None,
        })
        .await
        .expect_err("retry budget should block retry");

    match error {
        TauAiError::HttpStatus { status, body } => {
            assert_eq!(status, 429);
            assert!(body.contains("rate limited"));
        }
        other => panic!("expected TauAiError::HttpStatus, got {other:?}"),
    }

    first.assert_calls(1);
    second.assert_calls(0);
}

#[tokio::test]
async fn regression_openai_client_returns_timeout_error_when_server_is_slow() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST).path("/v1/chat/completions");
        then.status(200)
            .delay(Duration::from_millis(120))
            .json_body(json!({
                "choices": [{
                    "message": {"content": "late"},
                    "finish_reason": "stop"
                }],
                "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
            }));
    });

    let client = OpenAiClient::new(OpenAiConfig {
        api_base: format!("{}/v1", server.base_url()),
        api_key: "test-openai-key".to_string(),
        organization: None,
        request_timeout_ms: 40,
        max_retries: 2,
        retry_budget_ms: 0,
        retry_jitter: false,
        auth_scheme: OpenAiAuthScheme::Bearer,
        api_version: None,
    })
    .expect("openai client should be created");

    let error = client
        .complete(ChatRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![Message::user("hello")],
            tools: vec![],
            tool_choice: None,
            json_mode: false,
            max_tokens: None,
            temperature: None,
        })
        .await
        .expect_err("request should timeout");

    match error {
        TauAiError::Http(inner) => assert!(inner.is_timeout()),
        other => panic!("expected timeout HTTP error, got {other:?}"),
    }
}

#[tokio::test]
async fn integration_openai_client_streams_incremental_text_deltas() {
    let server = MockServer::start();
    let stream = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("x-tau-retry-attempt", "0")
            .json_body_includes(
                json!({
                    "model": "gpt-4o-mini",
                    "stream": true
                })
                .to_string(),
            );
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(concat!(
                "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"lo\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":2,\"total_tokens\":5}}\n\n",
                "data: [DONE]\n\n"
            ));
    });

    let client = OpenAiClient::new(OpenAiConfig {
        api_base: format!("{}/v1", server.base_url()),
        api_key: "test-openai-key".to_string(),
        organization: None,
        request_timeout_ms: 5_000,
        max_retries: 2,
        retry_budget_ms: 0,
        retry_jitter: false,
        auth_scheme: OpenAiAuthScheme::Bearer,
        api_version: None,
    })
    .expect("openai client should be created");

    let deltas = Arc::new(Mutex::new(String::new()));
    let delta_sink = deltas.clone();
    let sink = Arc::new(move |delta: String| {
        delta_sink.lock().expect("delta lock").push_str(&delta);
    });

    let response = client
        .complete_with_stream(
            ChatRequest {
                model: "gpt-4o-mini".to_string(),
                messages: vec![Message::user("hello")],
                tools: vec![],
                tool_choice: None,
                json_mode: false,
                max_tokens: None,
                temperature: None,
            },
            Some(sink),
        )
        .await
        .expect("streaming completion should succeed");

    stream.assert_calls(1);
    assert_eq!(deltas.lock().expect("delta lock").as_str(), "Hello");
    assert_eq!(response.message.text_content(), "Hello");
    assert_eq!(response.finish_reason.as_deref(), Some("stop"));
    assert_eq!(response.usage.total_tokens, 5);
}

#[tokio::test]
async fn integration_anthropic_client_streams_incremental_text_deltas() {
    let server = MockServer::start();
    let stream = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/messages")
            .header("x-tau-retry-attempt", "0")
            .json_body_includes(
                json!({
                    "model": "claude-sonnet-4-20250514",
                    "stream": true
                })
                .to_string(),
            );
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(concat!(
                "event: message_start\n",
                "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":6}}}\n\n",
                "event: content_block_delta\n",
                "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"He\"}}\n\n",
                "event: content_block_delta\n",
                "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"llo\"}}\n\n",
                "event: message_delta\n",
                "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":4}}\n\n",
                "event: message_stop\n",
                "data: {\"type\":\"message_stop\"}\n\n"
            ));
    });

    let client = AnthropicClient::new(AnthropicConfig {
        api_base: format!("{}/v1", server.base_url()),
        api_key: "test-anthropic-key".to_string(),
        request_timeout_ms: 5_000,
        max_retries: 2,
        retry_budget_ms: 0,
        retry_jitter: false,
    })
    .expect("anthropic client should be created");

    let deltas = Arc::new(Mutex::new(String::new()));
    let delta_sink = deltas.clone();
    let sink = Arc::new(move |delta: String| {
        delta_sink.lock().expect("delta lock").push_str(&delta);
    });

    let response = client
        .complete_with_stream(
            ChatRequest {
                model: "claude-sonnet-4-20250514".to_string(),
                messages: vec![Message::user("hello")],
                tools: vec![],
                tool_choice: None,
                json_mode: false,
                max_tokens: None,
                temperature: None,
            },
            Some(sink),
        )
        .await
        .expect("streaming completion should succeed");

    stream.assert_calls(1);
    assert_eq!(deltas.lock().expect("delta lock").as_str(), "Hello");
    assert_eq!(response.message.text_content(), "Hello");
    assert_eq!(response.finish_reason.as_deref(), Some("end_turn"));
    assert_eq!(response.usage.total_tokens, 10);
}

#[tokio::test]
async fn integration_google_client_streams_incremental_text_deltas() {
    let server = MockServer::start();
    let stream = server.mock(|when, then| {
        when.method(POST)
            .path("/models/gemini-2.5-pro:streamGenerateContent")
            .query_param("key", "test-google-key")
            .query_param("alt", "sse")
            .header("x-tau-retry-attempt", "0");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(concat!(
                "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"He\"}]}}]}\n\n",
                "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"llo\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":4,\"candidatesTokenCount\":3,\"totalTokenCount\":7}}\n\n"
            ));
    });

    let client = GoogleClient::new(GoogleConfig {
        api_base: server.base_url(),
        api_key: "test-google-key".to_string(),
        request_timeout_ms: 5_000,
        max_retries: 2,
        retry_budget_ms: 0,
        retry_jitter: false,
    })
    .expect("google client should be created");

    let deltas = Arc::new(Mutex::new(String::new()));
    let delta_sink = deltas.clone();
    let sink = Arc::new(move |delta: String| {
        delta_sink.lock().expect("delta lock").push_str(&delta);
    });

    let response = client
        .complete_with_stream(
            ChatRequest {
                model: "gemini-2.5-pro".to_string(),
                messages: vec![Message::user("hello")],
                tools: vec![],
                tool_choice: None,
                json_mode: false,
                max_tokens: None,
                temperature: None,
            },
            Some(sink),
        )
        .await
        .expect("streaming completion should succeed");

    stream.assert_calls(1);
    assert_eq!(deltas.lock().expect("delta lock").as_str(), "Hello");
    assert_eq!(response.message.text_content(), "Hello");
    assert_eq!(response.finish_reason.as_deref(), Some("STOP"));
    assert_eq!(response.usage.total_tokens, 7);
}
