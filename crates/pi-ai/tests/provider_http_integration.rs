use httpmock::prelude::*;
use pi_ai::{
    AnthropicClient, AnthropicConfig, ChatRequest, GoogleClient, GoogleConfig, LlmClient, Message,
    OpenAiClient, OpenAiConfig, PiAiError, ToolDefinition,
};
use serde_json::json;
use std::time::Duration;

#[tokio::test]
async fn openai_client_sends_expected_http_request() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .header_exists("x-pi-request-id")
            .header("x-pi-retry-attempt", "0")
            .json_body_includes(
                json!({
                    "model": "gpt-4o-mini",
                    "messages": [{"role": "system"}, {"role": "user"}],
                    "tools": [{"type": "function"}]
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
async fn anthropic_client_sends_expected_http_request() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/messages")
            .header("x-api-key", "test-anthropic-key")
            .header("anthropic-version", "2023-06-01")
            .header_exists("x-pi-request-id")
            .header("x-pi-retry-attempt", "0")
            .json_body_includes(
                json!({
                    "model": "claude-sonnet-4-20250514",
                    "system": "system",
                    "messages": [{"role": "user"}],
                    "tools": [{"name": "read"}]
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
            .header_exists("x-pi-request-id")
            .header("x-pi-retry-attempt", "0")
            .json_body_includes(
                json!({
                    "contents": [{"role": "user"}],
                    "tools": [{"functionDeclarations": [{"name": "read"}]}]
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
    })
    .expect("openai client should be created");

    let request = ChatRequest {
        model: "gpt-4o-mini".to_string(),
        messages: vec![Message::user("hello")],
        tools: vec![],
        max_tokens: None,
        temperature: None,
    };

    let error = client
        .complete(request)
        .await
        .expect_err("request should fail with 401");

    match error {
        PiAiError::HttpStatus { status, body } => {
            assert_eq!(status, 401);
            assert!(body.contains("unauthorized"));
        }
        other => panic!("expected PiAiError::HttpStatus, got {other:?}"),
    }
}

#[tokio::test]
async fn openai_client_retries_on_rate_limit_then_succeeds() {
    let server = MockServer::start();
    let first = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("x-pi-retry-attempt", "0");
        then.status(429).body("rate limited");
    });
    let second = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("x-pi-retry-attempt", "1");
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
    })
    .expect("openai client should be created");

    let response = client
        .complete(ChatRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![Message::user("hello")],
            tools: vec![],
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
    })
    .expect("openai client should be created");

    let error = client
        .complete(ChatRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![Message::user("hello")],
            tools: vec![],
            max_tokens: None,
            temperature: None,
        })
        .await
        .expect_err("request should timeout");

    match error {
        PiAiError::Http(inner) => assert!(inner.is_timeout()),
        other => panic!("expected timeout HTTP error, got {other:?}"),
    }
}
