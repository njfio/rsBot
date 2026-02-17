use httpmock::prelude::*;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tau_ai::{
    AnthropicClient, AnthropicConfig, ChatRequest, ChatResponse, GoogleClient, GoogleConfig,
    LlmClient, Message, OpenAiAuthScheme, OpenAiClient, OpenAiConfig, TauAiError, ToolChoice,
    ToolDefinition,
};

#[derive(Debug, Clone, PartialEq)]
struct NormalizedToolContract {
    text: String,
    tool_name: String,
    tool_arguments: Value,
    tool_count: usize,
    total_tokens: u64,
}

#[derive(Debug, Clone, PartialEq)]
struct NormalizedStreamContract {
    text: String,
    finish_reason: Option<String>,
    total_tokens: u64,
}

fn normalize_tool_contract(response: ChatResponse) -> NormalizedToolContract {
    let text = response.message.text_content();
    let tool_calls = response.message.tool_calls();
    let first_tool = tool_calls
        .first()
        .expect("tool-call contract expects at least one tool call");

    NormalizedToolContract {
        text,
        tool_name: first_tool.name.clone(),
        tool_arguments: first_tool.arguments.clone(),
        tool_count: tool_calls.len(),
        total_tokens: response.usage.total_tokens,
    }
}

fn normalize_stream_contract(response: ChatResponse) -> NormalizedStreamContract {
    NormalizedStreamContract {
        text: response.message.text_content(),
        finish_reason: response
            .finish_reason
            .map(|reason| reason.to_ascii_lowercase()),
        total_tokens: response.usage.total_tokens,
    }
}

fn tool_request(model: &str) -> ChatRequest {
    ChatRequest {
        model: model.to_string(),
        messages: vec![Message::system("system"), Message::user("hello")],
        tools: vec![ToolDefinition {
            name: "read".to_string(),
            description: "Read a file".to_string(),
            parameters: json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}),
        }],
        tool_choice: Some(ToolChoice::Auto),
        json_mode: false,
        max_tokens: Some(128),
        temperature: Some(0.0),
        prompt_cache: Default::default(),
    }
}

fn prompt_request(model: &str) -> ChatRequest {
    ChatRequest {
        model: model.to_string(),
        messages: vec![Message::user("hello")],
        tools: vec![],
        tool_choice: None,
        json_mode: false,
        max_tokens: None,
        temperature: None,
        prompt_cache: Default::default(),
    }
}

fn assert_serde_error(error: TauAiError) {
    match error {
        TauAiError::Serde(_) => {}
        other => panic!("expected TauAiError::Serde, got {other:?}"),
    }
}

fn openai_client(api_base: String) -> OpenAiClient {
    OpenAiClient::new(OpenAiConfig {
        api_base,
        api_key: "test-openai-key".to_string(),
        organization: None,
        request_timeout_ms: 5_000,
        max_retries: 1,
        retry_budget_ms: 0,
        retry_jitter: false,
        auth_scheme: OpenAiAuthScheme::Bearer,
        api_version: None,
    })
    .expect("openai client should be created")
}

fn anthropic_client(api_base: String) -> AnthropicClient {
    AnthropicClient::new(AnthropicConfig {
        api_base,
        api_key: "test-anthropic-key".to_string(),
        request_timeout_ms: 5_000,
        max_retries: 1,
        retry_budget_ms: 0,
        retry_jitter: false,
    })
    .expect("anthropic client should be created")
}

fn google_client(api_base: String) -> GoogleClient {
    GoogleClient::new(GoogleConfig {
        api_base,
        api_key: "test-google-key".to_string(),
        request_timeout_ms: 5_000,
        max_retries: 1,
        retry_budget_ms: 0,
        retry_jitter: false,
    })
    .expect("google client should be created")
}

#[test]
fn unit_normalize_tool_contract_extracts_stable_shape() {
    let response = ChatResponse {
        message: Message::assistant_blocks(vec![
            tau_ai::ContentBlock::Text {
                text: "contract ok".to_string(),
            },
            tau_ai::ContentBlock::ToolCall {
                id: "call_1".to_string(),
                name: "read".to_string(),
                arguments: json!({"path":"README.md"}),
            },
        ]),
        finish_reason: Some("stop".to_string()),
        usage: tau_ai::ChatUsage {
            input_tokens: 8,
            output_tokens: 6,
            total_tokens: 14,
            cached_input_tokens: 0,
        },
    };

    let normalized = normalize_tool_contract(response);
    assert_eq!(
        normalized,
        NormalizedToolContract {
            text: "contract ok".to_string(),
            tool_name: "read".to_string(),
            tool_arguments: json!({"path":"README.md"}),
            tool_count: 1,
            total_tokens: 14,
        }
    );
}

#[tokio::test]
async fn functional_non_stream_tool_contract_is_normalized_across_providers() {
    let openai_server = MockServer::start();
    let anthropic_server = MockServer::start();
    let google_server = MockServer::start();

    let openai_mock = openai_server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {
                    "content": "contract ok",
                    "tool_calls": [{
                        "id": "call_openai_1",
                        "type": "function",
                        "function": {
                            "name": "read",
                            "arguments": "{\"path\":\"README.md\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {
                "prompt_tokens": 8,
                "completion_tokens": 6,
                "total_tokens": 14
            }
        }));
    });

    let anthropic_mock = anthropic_server.mock(|when, then| {
        when.method(POST)
            .path("/v1/messages")
            .header("x-api-key", "test-anthropic-key");
        then.status(200).json_body(json!({
            "content": [
                {"type":"text","text":"contract ok"},
                {"type":"tool_use","id":"toolu_1","name":"read","input":{"path":"README.md"}}
            ],
            "stop_reason": "tool_use",
            "usage": {
                "input_tokens": 8,
                "output_tokens": 6
            }
        }));
    });

    let google_mock = google_server.mock(|when, then| {
        when.method(POST)
            .path("/models/gemini-2.5-pro:generateContent")
            .query_param("key", "test-google-key");
        then.status(200).json_body(json!({
            "candidates": [{
                "content": {
                    "parts": [
                        {"text": "contract ok"},
                        {"functionCall": {"name": "read", "args": {"path": "README.md"}}}
                    ]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 8,
                "candidatesTokenCount": 6,
                "totalTokenCount": 14
            }
        }));
    });

    let openai_response = openai_client(format!("{}/v1", openai_server.base_url()))
        .complete(tool_request("gpt-4o-mini"))
        .await
        .expect("openai contract request should succeed");
    let anthropic_response = anthropic_client(format!("{}/v1", anthropic_server.base_url()))
        .complete(tool_request("claude-sonnet-4-20250514"))
        .await
        .expect("anthropic contract request should succeed");
    let google_response = google_client(google_server.base_url())
        .complete(tool_request("gemini-2.5-pro"))
        .await
        .expect("google contract request should succeed");

    openai_mock.assert_calls(1);
    anthropic_mock.assert_calls(1);
    google_mock.assert_calls(1);

    let normalized_openai = normalize_tool_contract(openai_response);
    let normalized_anthropic = normalize_tool_contract(anthropic_response);
    let normalized_google = normalize_tool_contract(google_response);

    assert_eq!(normalized_openai, normalized_anthropic);
    assert_eq!(normalized_anthropic, normalized_google);
    assert_eq!(normalized_openai.tool_name, "read");
    assert_eq!(
        normalized_openai.tool_arguments,
        json!({"path":"README.md"})
    );
    assert_eq!(normalized_openai.total_tokens, 14);
}

#[tokio::test]
async fn integration_stream_contract_is_normalized_across_providers() {
    let openai_server = MockServer::start();
    let anthropic_server = MockServer::start();
    let google_server = MockServer::start();

    let openai_stream = openai_server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("x-tau-retry-attempt", "0")
            .json_body_includes(json!({"stream": true}).to_string());
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(concat!(
                "data: {\"choices\":[{\"delta\":{\"content\":\"Con\"}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"tract\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":3,\"total_tokens\":7}}\n\n",
                "data: [DONE]\n\n"
            ));
    });

    let anthropic_stream = anthropic_server.mock(|when, then| {
        when.method(POST)
            .path("/v1/messages")
            .header("x-tau-retry-attempt", "0")
            .json_body_includes(json!({"stream": true}).to_string());
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(concat!(
                "event: message_start\n",
                "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":4}}}\n\n",
                "event: content_block_delta\n",
                "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Con\"}}\n\n",
                "event: content_block_delta\n",
                "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"tract\"}}\n\n",
                "event: message_delta\n",
                "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"stop\"},\"usage\":{\"output_tokens\":3}}\n\n",
                "event: message_stop\n",
                "data: {\"type\":\"message_stop\"}\n\n"
            ));
    });

    let google_stream = google_server.mock(|when, then| {
        when.method(POST)
            .path("/models/gemini-2.5-pro:streamGenerateContent")
            .query_param("key", "test-google-key")
            .query_param("alt", "sse")
            .header("x-tau-retry-attempt", "0");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(concat!(
                "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Con\"}]}}]}\n\n",
                "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"tract\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":4,\"candidatesTokenCount\":3,\"totalTokenCount\":7}}\n\n"
            ));
    });

    let openai_deltas = Arc::new(Mutex::new(String::new()));
    let openai_sink = {
        let deltas = openai_deltas.clone();
        Arc::new(move |delta: String| {
            deltas.lock().expect("openai delta lock").push_str(&delta);
        })
    };
    let anthropic_deltas = Arc::new(Mutex::new(String::new()));
    let anthropic_sink = {
        let deltas = anthropic_deltas.clone();
        Arc::new(move |delta: String| {
            deltas
                .lock()
                .expect("anthropic delta lock")
                .push_str(&delta);
        })
    };
    let google_deltas = Arc::new(Mutex::new(String::new()));
    let google_sink = {
        let deltas = google_deltas.clone();
        Arc::new(move |delta: String| {
            deltas.lock().expect("google delta lock").push_str(&delta);
        })
    };

    let openai_response = openai_client(format!("{}/v1", openai_server.base_url()))
        .complete_with_stream(prompt_request("gpt-4o-mini"), Some(openai_sink))
        .await
        .expect("openai stream contract should succeed");
    let anthropic_response = anthropic_client(format!("{}/v1", anthropic_server.base_url()))
        .complete_with_stream(
            prompt_request("claude-sonnet-4-20250514"),
            Some(anthropic_sink),
        )
        .await
        .expect("anthropic stream contract should succeed");
    let google_response = google_client(google_server.base_url())
        .complete_with_stream(prompt_request("gemini-2.5-pro"), Some(google_sink))
        .await
        .expect("google stream contract should succeed");

    openai_stream.assert_calls(1);
    anthropic_stream.assert_calls(1);
    google_stream.assert_calls(1);

    assert_eq!(
        openai_deltas.lock().expect("openai delta lock").as_str(),
        "Contract"
    );
    assert_eq!(
        anthropic_deltas
            .lock()
            .expect("anthropic delta lock")
            .as_str(),
        "Contract"
    );
    assert_eq!(
        google_deltas.lock().expect("google delta lock").as_str(),
        "Contract"
    );

    let normalized_openai = normalize_stream_contract(openai_response);
    let normalized_anthropic = normalize_stream_contract(anthropic_response);
    let normalized_google = normalize_stream_contract(google_response);

    assert_eq!(normalized_openai, normalized_anthropic);
    assert_eq!(normalized_anthropic, normalized_google);
    assert_eq!(
        normalized_openai,
        NormalizedStreamContract {
            text: "Contract".to_string(),
            finish_reason: Some("stop".to_string()),
            total_tokens: 7,
        }
    );
}

#[tokio::test]
async fn regression_malformed_payloads_return_structured_parse_errors() {
    let openai_server = MockServer::start();
    let anthropic_server = MockServer::start();
    let google_server = MockServer::start();

    openai_server.mock(|when, then| {
        when.method(POST).path("/v1/chat/completions");
        then.status(200)
            .header("content-type", "application/json")
            .body("{not-json");
    });

    anthropic_server.mock(|when, then| {
        when.method(POST).path("/v1/messages");
        then.status(200)
            .header("content-type", "application/json")
            .body("{not-json");
    });

    google_server.mock(|when, then| {
        when.method(POST)
            .path("/models/gemini-2.5-pro:generateContent")
            .query_param("key", "test-google-key");
        then.status(200)
            .header("content-type", "application/json")
            .body("{not-json");
    });

    let openai_error = openai_client(format!("{}/v1", openai_server.base_url()))
        .complete(prompt_request("gpt-4o-mini"))
        .await
        .expect_err("openai malformed payload should fail");
    let anthropic_error = anthropic_client(format!("{}/v1", anthropic_server.base_url()))
        .complete(prompt_request("claude-sonnet-4-20250514"))
        .await
        .expect_err("anthropic malformed payload should fail");
    let google_error = google_client(google_server.base_url())
        .complete(prompt_request("gemini-2.5-pro"))
        .await
        .expect_err("google malformed payload should fail");

    assert_serde_error(openai_error);
    assert_serde_error(anthropic_error);
    assert_serde_error(google_error);
}
