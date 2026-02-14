use super::*;

#[tokio::test]
async fn functional_prompt_returns_cancelled_when_token_is_pre_cancelled() {
    let client = Arc::new(CapturingMockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: Message::assistant_text("should not be used"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }])),
        requests: AsyncMutex::new(Vec::new()),
    });
    let mut agent = Agent::new(client.clone(), AgentConfig::default());
    let token = CooperativeCancellationToken::new();
    token.cancel();
    agent.set_cancellation_token(Some(token));

    let error = agent
        .prompt("hello")
        .await
        .expect_err("prompt should cancel");
    assert!(matches!(error, AgentError::Cancelled));
    assert_eq!(client.requests.lock().await.len(), 0);
}

#[test]
fn unit_async_event_metrics_default_to_zero() {
    let agent = Agent::new(Arc::new(EchoClient), AgentConfig::default());
    assert_eq!(
        agent.async_event_metrics(),
        AsyncEventDispatchMetrics::default()
    );
}

#[tokio::test]
async fn turn_end_events_include_usage_finish_reason_and_request_duration() {
    let usage = ChatUsage {
        input_tokens: 3,
        output_tokens: 2,
        total_tokens: 5,
    };
    let client = Arc::new(MockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: Message::assistant_text("done"),
            finish_reason: Some("stop".to_string()),
            usage: usage.clone(),
        }])),
    });

    let mut agent = Agent::new(client, AgentConfig::default());
    let turn_ends = Arc::new(Mutex::new(Vec::<(
        usize,
        usize,
        u64,
        ChatUsage,
        Option<String>,
    )>::new()));
    let captured = turn_ends.clone();
    agent.subscribe(move |event| {
        if let AgentEvent::TurnEnd {
            turn,
            tool_results,
            request_duration_ms,
            usage,
            finish_reason,
        } = event
        {
            captured.lock().expect("turn_end lock").push((
                *turn,
                *tool_results,
                *request_duration_ms,
                usage.clone(),
                finish_reason.clone(),
            ));
        }
    });

    let _ = agent.prompt("hello").await.expect("prompt should succeed");

    let turn_ends = turn_ends.lock().expect("turn_end lock");
    assert_eq!(turn_ends.len(), 1);
    assert_eq!(turn_ends[0].0, 1);
    assert_eq!(turn_ends[0].1, 0);
    assert_eq!(turn_ends[0].3, usage);
    assert_eq!(turn_ends[0].4.as_deref(), Some("stop"));
}

#[test]
fn unit_extract_json_payload_parses_plain_and_fenced_json() {
    let plain = extract_json_payload(r#"{"ok":true,"count":2}"#).expect("plain json");
    assert_eq!(plain["ok"], true);
    assert_eq!(plain["count"], 2);

    let fenced = extract_json_payload(
        "result follows\n```json\n{\"status\":\"pass\",\"items\":[1,2]}\n```\nthanks",
    )
    .expect("fenced json");
    assert_eq!(fenced["status"], "pass");
    assert_eq!(fenced["items"][1], 2);
}

#[test]
fn unit_assistant_text_suggests_failure_matches_common_markers() {
    assert!(assistant_text_suggests_failure(
        "Unable to continue after the error."
    ));
    assert!(assistant_text_suggests_failure(
        "I can't proceed with this tool."
    ));
    assert!(assistant_text_suggests_failure("   "));
    assert!(!assistant_text_suggests_failure("Completed successfully."));
}

#[tokio::test]
async fn unit_vector_retrieval_prefers_semantically_related_entries() {
    let history = vec![
        Message::user("rust tokio async runtime troubleshooting"),
        Message::assistant_text("pasta recipe with basil and tomato"),
    ];
    let matches = retrieve_memory_matches(
        &history,
        "tokio runtime async rust",
        1,
        64,
        0.0,
        &AgentConfig::default(),
    )
    .await;
    assert_eq!(matches.len(), 1);
    assert!(matches[0].text.contains("tokio"));

    let query = embed_text_vector("tokio runtime async rust", 64);
    let related = embed_text_vector("rust tokio async runtime troubleshooting", 64);
    let unrelated = embed_text_vector("pasta recipe with basil and tomato", 64);
    let related_score = query
        .iter()
        .zip(&related)
        .map(|(left, right)| left * right)
        .sum::<f32>();
    let unrelated_score = query
        .iter()
        .zip(&unrelated)
        .map(|(left, right)| left * right)
        .sum::<f32>();
    assert!(related_score > unrelated_score);
}

#[tokio::test]
async fn integration_memory_retrieval_uses_embedding_api_when_configured() {
    let server = MockServer::start();
    let embedding_mock = server.mock(|when, then| {
        when.method(POST).path("/embeddings");
        then.status(200)
            .header("content-type", "application/json")
            .body(
                r#"{
  "data": [
{ "embedding": [1.0, 0.0, 0.0] },
{ "embedding": [0.95, 0.05, 0.0] },
{ "embedding": [0.0, 1.0, 0.0] }
  ]
}"#,
            );
    });

    let history = vec![
        Message::assistant_text("retry strategy for postgres in payments service"),
        Message::assistant_text("fresh tomato pasta recipe"),
    ];
    let config = AgentConfig {
        memory_embedding_dimensions: 3,
        memory_embedding_model: Some("text-embedding-3-small".to_string()),
        memory_embedding_api_base: Some(server.url("")),
        memory_embedding_api_key: Some("test-key".to_string()),
        ..AgentConfig::default()
    };

    let matches = retrieve_memory_matches(
        &history,
        "payments postgres retry policy",
        1,
        3,
        0.2,
        &config,
    )
    .await;

    embedding_mock.assert();
    assert_eq!(matches.len(), 1);
    assert!(matches[0].text.contains("payments"));
    assert!(matches[0].text.contains("postgres"));
}

#[tokio::test]
async fn regression_memory_retrieval_falls_back_to_hash_when_embedding_api_fails() {
    let server = MockServer::start();
    let embedding_mock = server.mock(|when, then| {
        when.method(POST).path("/embeddings");
        then.status(500);
    });

    let history = vec![
        Message::assistant_text("tokio runtime troubleshooting checklist"),
        Message::assistant_text("basil and tomato pasta"),
    ];
    let config = AgentConfig {
        memory_embedding_dimensions: 64,
        memory_embedding_model: Some("text-embedding-3-small".to_string()),
        memory_embedding_api_base: Some(server.url("")),
        memory_embedding_api_key: Some("test-key".to_string()),
        ..AgentConfig::default()
    };

    let matches = retrieve_memory_matches(&history, "tokio runtime", 1, 64, 0.0, &config).await;

    embedding_mock.assert();
    assert_eq!(matches.len(), 1);
    assert!(matches[0].text.contains("tokio"));
}

#[tokio::test]
async fn functional_prompt_json_returns_validated_value() {
    let client = Arc::new(MockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: Message::assistant_text("{\"tasks\":[\"a\",\"b\"],\"ok\":true}"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }])),
    });
    let mut agent = Agent::new(client, AgentConfig::default());
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "tasks": {
                "type": "array",
                "items": { "type": "string" }
            },
            "ok": { "type": "boolean" }
        },
        "required": ["tasks", "ok"],
        "additionalProperties": false
    });

    let value = agent
        .prompt_json("return tasks", &schema)
        .await
        .expect("structured output should succeed");
    assert_eq!(value["ok"], true);
    assert_eq!(value["tasks"][0], "a");
}

#[tokio::test]
async fn functional_prompt_json_retries_after_non_json_and_succeeds() {
    let client = Arc::new(CapturingMockClient {
        responses: AsyncMutex::new(VecDeque::from([
            ChatResponse {
                message: Message::assistant_text("not-json-response"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: Message::assistant_text("{\"tasks\":[\"retry\"],\"ok\":true}"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
        ])),
        requests: AsyncMutex::new(Vec::new()),
    });
    let mut agent = Agent::new(
        client.clone(),
        AgentConfig {
            structured_output_max_retries: 1,
            ..AgentConfig::default()
        },
    );
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "tasks": {
                "type": "array",
                "items": { "type": "string" }
            },
            "ok": { "type": "boolean" }
        },
        "required": ["tasks", "ok"],
        "additionalProperties": false
    });

    let value = agent
        .prompt_json("return tasks", &schema)
        .await
        .expect("structured output retry should recover");
    assert_eq!(value["ok"], true);
    assert_eq!(value["tasks"][0], "retry");

    let requests = client.requests.lock().await;
    assert_eq!(requests.len(), 2, "prompt_json should perform one retry");
    let retry_prompt = last_user_prompt(&requests[1]);
    assert!(retry_prompt.contains("could not be accepted as structured JSON"));
    assert!(retry_prompt.contains("\"tasks\""));
}

#[tokio::test]
async fn functional_prompt_json_enables_provider_json_mode_on_requests() {
    let client = Arc::new(CapturingMockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: Message::assistant_text("{\"ok\":true}"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }])),
        requests: AsyncMutex::new(Vec::new()),
    });
    let mut agent = Agent::new(client.clone(), AgentConfig::default());
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "ok": { "type": "boolean" }
        },
        "required": ["ok"],
        "additionalProperties": false
    });

    let value = agent
        .prompt_json("return ok", &schema)
        .await
        .expect("structured output should succeed");
    assert_eq!(value["ok"], true);

    let requests = client.requests.lock().await;
    assert_eq!(requests.len(), 1);
    assert!(requests[0].json_mode);
}

#[tokio::test]
async fn integration_prompt_json_accepts_fenced_json_payload() {
    let client = Arc::new(MockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: Message::assistant_text(
                "Here is the payload:\n```json\n{\"mode\":\"apply\",\"steps\":3}\n```",
            ),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }])),
    });
    let mut agent = Agent::new(client, AgentConfig::default());
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "mode": { "type": "string" },
            "steps": { "type": "integer" }
        },
        "required": ["mode", "steps"]
    });

    let value = agent
        .prompt_json("return mode", &schema)
        .await
        .expect("fenced structured output should parse");
    assert_eq!(value["mode"], "apply");
    assert_eq!(value["steps"], 3);
}

#[tokio::test]
async fn integration_continue_turn_json_retries_after_schema_failure_and_succeeds() {
    let client = Arc::new(CapturingMockClient {
        responses: AsyncMutex::new(VecDeque::from([
            ChatResponse {
                message: Message::assistant_text("{\"mode\":\"apply\"}"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: Message::assistant_text("{\"mode\":\"apply\",\"steps\":2}"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
        ])),
        requests: AsyncMutex::new(Vec::new()),
    });
    let mut agent = Agent::new(
        client.clone(),
        AgentConfig {
            structured_output_max_retries: 1,
            ..AgentConfig::default()
        },
    );
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "mode": { "type": "string" },
            "steps": { "type": "integer" }
        },
        "required": ["mode", "steps"]
    });

    let value = agent
        .continue_turn_json(&schema)
        .await
        .expect("continue_turn_json should recover via retry");
    assert_eq!(value["mode"], "apply");
    assert_eq!(value["steps"], 2);

    let requests = client.requests.lock().await;
    assert_eq!(
        requests.len(),
        2,
        "continue_turn_json should perform one retry request"
    );
    let retry_prompt = last_user_prompt(&requests[1]);
    assert!(retry_prompt.contains("schema validation failed"));
    assert!(retry_prompt.contains("\"steps\""));
}

#[tokio::test]
async fn integration_requests_with_registered_tools_use_auto_tool_choice() {
    let client = Arc::new(CapturingMockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: Message::assistant_text("done"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }])),
        requests: AsyncMutex::new(Vec::new()),
    });
    let mut agent = Agent::new(client.clone(), AgentConfig::default());
    agent.register_tool(ReadTool);

    let _ = agent.prompt("hello").await.expect("prompt should succeed");

    let requests = client.requests.lock().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].tool_choice, Some(ToolChoice::Auto));
    assert!(!requests[0].json_mode);
}

#[tokio::test]
async fn regression_prompt_json_fails_closed_on_non_json_response() {
    let client = Arc::new(MockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: Message::assistant_text("not-json-response"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }])),
    });
    let mut agent = Agent::new(
        client,
        AgentConfig {
            structured_output_max_retries: 0,
            ..AgentConfig::default()
        },
    );
    let schema = serde_json::json!({ "type": "object" });

    let error = agent
        .prompt_json("return object", &schema)
        .await
        .expect_err("non-json output must fail");
    assert!(matches!(error, AgentError::StructuredOutput(_)));
    assert!(error.to_string().contains("did not contain parseable JSON"));
}

#[tokio::test]
async fn regression_prompt_json_fails_closed_on_schema_mismatch() {
    let client = Arc::new(MockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: Message::assistant_text("{\"ok\":true}"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }])),
    });
    let mut agent = Agent::new(
        client,
        AgentConfig {
            structured_output_max_retries: 0,
            ..AgentConfig::default()
        },
    );
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "ok": { "type": "boolean" },
            "tasks": { "type": "array" }
        },
        "required": ["ok", "tasks"]
    });

    let error = agent
        .prompt_json("return object", &schema)
        .await
        .expect_err("schema mismatch must fail");
    assert!(matches!(error, AgentError::StructuredOutput(_)));
    assert!(error.to_string().contains("schema validation failed"));
}

#[tokio::test]
async fn regression_requests_without_tools_keep_tool_choice_unset() {
    let client = Arc::new(CapturingMockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: Message::assistant_text("done"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }])),
        requests: AsyncMutex::new(Vec::new()),
    });
    let mut agent = Agent::new(client.clone(), AgentConfig::default());

    let _ = agent.prompt("hello").await.expect("prompt should succeed");

    let requests = client.requests.lock().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].tool_choice, None);
}

#[tokio::test]
async fn integration_tool_execution_cancellation_propagates_as_agent_cancelled() {
    let first_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
        id: "call_1".to_string(),
        name: "slow_read".to_string(),
        arguments: serde_json::json!({
            "path": "README.md"
        }),
    }]);
    let client = Arc::new(MockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: first_assistant,
            finish_reason: Some("tool_calls".to_string()),
            usage: ChatUsage::default(),
        }])),
    });
    let mut agent = Agent::new(client, AgentConfig::default());
    agent.register_tool(SlowReadTool { delay_ms: 500 });
    let token = CooperativeCancellationToken::new();
    agent.set_cancellation_token(Some(token.clone()));

    let cancel_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        token.cancel();
    });

    let error = agent
        .prompt("read with cancellation")
        .await
        .expect_err("prompt should cancel cooperatively");
    assert!(
        matches!(error, AgentError::Cancelled),
        "expected AgentError::Cancelled, got {error:?}"
    );
    cancel_task.await.expect("cancel task should complete");
}

#[tokio::test]
async fn regression_agent_can_continue_after_cancellation_token_is_cleared() {
    let client = Arc::new(MockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: Message::assistant_text("ok"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }])),
    });
    let mut agent = Agent::new(client, AgentConfig::default());
    let token = CooperativeCancellationToken::new();
    token.cancel();
    agent.set_cancellation_token(Some(token));

    let error = agent
        .prompt("cancelled run")
        .await
        .expect_err("first prompt should be cancelled");
    assert!(matches!(error, AgentError::Cancelled));

    agent.set_cancellation_token(None);
    let new_messages = agent
        .prompt("second run")
        .await
        .expect("agent should continue once token is cleared");
    assert_eq!(
        new_messages
            .last()
            .expect("assistant response should exist")
            .text_content(),
        "ok"
    );
}

#[tokio::test]
async fn regression_continue_turn_json_fails_closed_when_assistant_lacks_json() {
    let client = Arc::new(MockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: Message::assistant_text("ack"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }])),
    });
    let mut agent = Agent::new(
        client,
        AgentConfig {
            structured_output_max_retries: 0,
            ..AgentConfig::default()
        },
    );
    let schema = serde_json::json!({ "type": "object" });

    let error = agent
        .continue_turn_json(&schema)
        .await
        .expect_err("missing json must fail");
    assert!(matches!(error, AgentError::StructuredOutput(_)));
}

#[tokio::test]
async fn functional_replan_prompt_injected_after_failed_tool_and_failure_response() {
    let first_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
        id: "call_1".to_string(),
        name: "read".to_string(),
        arguments: serde_json::json!({}),
    }]);
    let second_assistant = Message::assistant_text("I cannot continue because the tool failed.");
    let third_assistant = Message::assistant_text("recovered");
    let client = Arc::new(CapturingMockClient {
        responses: AsyncMutex::new(VecDeque::from([
            ChatResponse {
                message: first_assistant,
                finish_reason: Some("tool_calls".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: second_assistant,
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: third_assistant,
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
        ])),
        requests: AsyncMutex::new(Vec::new()),
    });
    let mut agent = Agent::new(
        client.clone(),
        AgentConfig {
            react_max_replans_on_tool_failure: 1,
            ..AgentConfig::default()
        },
    );
    agent.register_tool(ReadTool);
    let replan_count = Arc::new(AtomicUsize::new(0));
    let replan_count_sink = replan_count.clone();
    agent.subscribe(move |event| {
        if matches!(event, AgentEvent::ReplanTriggered { .. }) {
            replan_count_sink.fetch_add(1, Ordering::Relaxed);
        }
    });

    let messages = agent
        .prompt("read")
        .await
        .expect("replan flow should recover");
    assert_eq!(
        messages.last().expect("assistant response").text_content(),
        "recovered"
    );
    assert_eq!(replan_count.load(Ordering::Relaxed), 1);

    let requests = client.requests.lock().await;
    assert_eq!(
        requests.len(),
        3,
        "expected replan to trigger an extra turn"
    );
    let replan_prompt = last_user_prompt(&requests[2]);
    assert!(replan_prompt.contains("One or more tool calls failed"));
}

#[tokio::test]
async fn functional_request_messages_attach_memory_recall_for_relevant_history() {
    let client = Arc::new(CapturingMockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: Message::assistant_text("ok"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }])),
        requests: AsyncMutex::new(Vec::new()),
    });
    let mut agent = Agent::new(
        client.clone(),
        AgentConfig {
            max_context_messages: Some(4),
            memory_retrieval_limit: 2,
            memory_embedding_dimensions: 64,
            memory_min_similarity: 0.2,
            ..AgentConfig::default()
        },
    );
    agent.append_message(Message::user(
        "postgres retry configuration for orders service",
    ));
    agent.append_message(Message::assistant_text(
        "increase postgres pool size for orders workloads",
    ));
    agent.append_message(Message::user("cache ttl cleanup"));
    agent.append_message(Message::assistant_text("set ttl to 15m"));

    let _ = agent
        .prompt("postgres orders service retry policy")
        .await
        .expect("prompt should succeed");

    let requests = client.requests.lock().await;
    let first_request = requests.first().expect("request should be captured");
    let recall = first_request
        .messages
        .iter()
        .find(|message| {
            message.role == MessageRole::System
                && message.text_content().starts_with(MEMORY_RECALL_PREFIX)
        })
        .expect("memory recall system message should be attached");
    assert!(recall.text_content().contains("postgres"));
    assert!(recall.text_content().contains("orders"));
}

#[tokio::test]
async fn integration_parallel_tool_execution_runs_calls_concurrently_and_preserves_order() {
    let first_assistant = Message::assistant_blocks(vec![
        ContentBlock::ToolCall {
            id: "call_1".to_string(),
            name: "slow_read".to_string(),
            arguments: serde_json::json!({ "path": "a.txt" }),
        },
        ContentBlock::ToolCall {
            id: "call_2".to_string(),
            name: "slow_read".to_string(),
            arguments: serde_json::json!({ "path": "b.txt" }),
        },
    ]);
    let second_assistant = Message::assistant_text("done");
    let client = Arc::new(MockClient {
        responses: AsyncMutex::new(VecDeque::from([
            ChatResponse {
                message: first_assistant,
                finish_reason: Some("tool_calls".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: second_assistant,
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
        ])),
    });

    let mut agent = Agent::new(
        client,
        AgentConfig {
            max_parallel_tool_calls: 2,
            ..AgentConfig::default()
        },
    );
    agent.register_tool(SlowReadTool { delay_ms: 120 });

    let started = Instant::now();
    let messages = agent
        .prompt("read both")
        .await
        .expect("prompt should succeed");
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_millis(230),
        "expected concurrent tool execution under 230ms, got {elapsed:?}"
    );

    let tool_messages = messages
        .iter()
        .filter(|message| message.role == MessageRole::Tool)
        .collect::<Vec<_>>();
    assert_eq!(tool_messages.len(), 2);
    assert!(tool_messages[0].text_content().contains("read:a.txt"));
    assert!(tool_messages[1].text_content().contains("read:b.txt"));
}

#[tokio::test]
async fn integration_parallel_tool_cache_reuses_results_across_chunks() {
    let first_assistant = Message::assistant_blocks(vec![
        ContentBlock::ToolCall {
            id: "call_1".to_string(),
            name: "counting_read".to_string(),
            arguments: serde_json::json!({ "path": "a.txt" }),
        },
        ContentBlock::ToolCall {
            id: "call_2".to_string(),
            name: "counting_read".to_string(),
            arguments: serde_json::json!({ "path": "b.txt" }),
        },
        ContentBlock::ToolCall {
            id: "call_3".to_string(),
            name: "counting_read".to_string(),
            arguments: serde_json::json!({ "path": "a.txt" }),
        },
    ]);
    let second_assistant = Message::assistant_text("done");
    let client = Arc::new(MockClient {
        responses: AsyncMutex::new(VecDeque::from([
            ChatResponse {
                message: first_assistant,
                finish_reason: Some("tool_calls".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: second_assistant,
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
        ])),
    });
    let calls = Arc::new(AtomicUsize::new(0));
    let mut agent = Agent::new(
        client,
        AgentConfig {
            max_parallel_tool_calls: 2,
            ..AgentConfig::default()
        },
    );
    agent.register_tool(CountingReadTool {
        calls: calls.clone(),
        cacheable: true,
    });

    let messages = agent
        .prompt("read all")
        .await
        .expect("prompt should succeed");
    let tool_messages = messages
        .iter()
        .filter(|message| message.role == MessageRole::Tool)
        .collect::<Vec<_>>();
    assert_eq!(tool_messages.len(), 3);
    assert!(tool_messages[0].text_content().contains("counting:a.txt"));
    assert!(tool_messages[1].text_content().contains("counting:b.txt"));
    assert!(tool_messages[2].text_content().contains("counting:a.txt"));
    assert_eq!(calls.load(Ordering::Relaxed), 2);
}

#[tokio::test]
async fn regression_non_cacheable_tool_executes_each_identical_call() {
    let first_assistant = Message::assistant_blocks(vec![
        ContentBlock::ToolCall {
            id: "call_1".to_string(),
            name: "counting_read".to_string(),
            arguments: serde_json::json!({ "path": "a.txt" }),
        },
        ContentBlock::ToolCall {
            id: "call_2".to_string(),
            name: "counting_read".to_string(),
            arguments: serde_json::json!({ "path": "a.txt" }),
        },
    ]);
    let second_assistant = Message::assistant_text("done");
    let client = Arc::new(MockClient {
        responses: AsyncMutex::new(VecDeque::from([
            ChatResponse {
                message: first_assistant,
                finish_reason: Some("tool_calls".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: second_assistant,
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
        ])),
    });
    let calls = Arc::new(AtomicUsize::new(0));
    let mut agent = Agent::new(
        client,
        AgentConfig {
            max_parallel_tool_calls: 1,
            ..AgentConfig::default()
        },
    );
    agent.register_tool(CountingReadTool {
        calls: calls.clone(),
        cacheable: false,
    });

    let _ = agent
        .prompt("read duplicate")
        .await
        .expect("prompt should succeed");
    assert_eq!(calls.load(Ordering::Relaxed), 2);
}

#[tokio::test]
async fn integration_replan_flow_can_recover_with_follow_up_tool_call() {
    let first_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
        id: "call_1".to_string(),
        name: "read".to_string(),
        arguments: serde_json::json!({}),
    }]);
    let second_assistant = Message::assistant_text("Unable to continue after that tool error.");
    let third_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
        id: "call_2".to_string(),
        name: "read".to_string(),
        arguments: serde_json::json!({ "path": "README.md" }),
    }]);
    let fourth_assistant = Message::assistant_text("done after replan");
    let client = Arc::new(MockClient {
        responses: AsyncMutex::new(VecDeque::from([
            ChatResponse {
                message: first_assistant,
                finish_reason: Some("tool_calls".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: second_assistant,
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: third_assistant,
                finish_reason: Some("tool_calls".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: fourth_assistant,
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
        ])),
    });
    let mut agent = Agent::new(
        client,
        AgentConfig {
            react_max_replans_on_tool_failure: 1,
            ..AgentConfig::default()
        },
    );
    agent.register_tool(ReadTool);

    let messages = agent
        .prompt("read")
        .await
        .expect("replan flow should recover with second tool call");
    let tool_messages = messages
        .iter()
        .filter(|message| message.role == MessageRole::Tool)
        .collect::<Vec<_>>();
    assert_eq!(tool_messages.len(), 2);
    assert!(tool_messages[0].is_error);
    assert!(!tool_messages[1].is_error);
    assert_eq!(
        messages.last().expect("assistant response").text_content(),
        "done after replan"
    );
}

#[tokio::test]
async fn integration_memory_recall_ranks_relevant_entries_ahead_of_unrelated_entries() {
    let client = Arc::new(CapturingMockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: Message::assistant_text("ok"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }])),
        requests: AsyncMutex::new(Vec::new()),
    });
    let mut agent = Agent::new(
        client.clone(),
        AgentConfig {
            max_context_messages: Some(2),
            memory_retrieval_limit: 1,
            memory_embedding_dimensions: 64,
            memory_min_similarity: 0.1,
            ..AgentConfig::default()
        },
    );
    agent.append_message(Message::user("rust tokio runtime diagnostics"));
    agent.append_message(Message::user("pasta recipe tomato basil"));
    agent.append_message(Message::assistant_text("acknowledged"));

    let _ = agent
        .prompt("tokio runtime troubleshooting")
        .await
        .expect("prompt should succeed");

    let requests = client.requests.lock().await;
    let first_request = requests.first().expect("request should be captured");
    let recall = first_request
        .messages
        .iter()
        .find(|message| {
            message.role == MessageRole::System
                && message.text_content().starts_with(MEMORY_RECALL_PREFIX)
        })
        .expect("memory recall message should exist");
    assert!(recall.text_content().contains("tokio"));
    assert!(!recall.text_content().contains("pasta recipe"));
}

#[tokio::test]
async fn regression_bug_1_max_parallel_tool_calls_zero_clamps_to_safe_serial_execution() {
    let first_assistant = Message::assistant_blocks(vec![
        ContentBlock::ToolCall {
            id: "call_1".to_string(),
            name: "read".to_string(),
            arguments: serde_json::json!({ "path": "a.txt" }),
        },
        ContentBlock::ToolCall {
            id: "call_2".to_string(),
            name: "read".to_string(),
            arguments: serde_json::json!({ "path": "b.txt" }),
        },
    ]);
    let second_assistant = Message::assistant_text("done");
    let client = Arc::new(MockClient {
        responses: AsyncMutex::new(VecDeque::from([
            ChatResponse {
                message: first_assistant,
                finish_reason: Some("tool_calls".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: second_assistant,
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
        ])),
    });

    let mut agent = Agent::new(
        client,
        AgentConfig {
            max_parallel_tool_calls: 0,
            ..AgentConfig::default()
        },
    );
    agent.register_tool(ReadTool);

    let messages = agent
        .prompt("read both")
        .await
        .expect("zero parallel limit should be normalized to a safe value");
    let tool_messages = messages
        .iter()
        .filter(|message| message.role == MessageRole::Tool)
        .collect::<Vec<_>>();
    assert_eq!(tool_messages.len(), 2);
    assert!(tool_messages[0].text_content().contains("read:a.txt"));
    assert!(tool_messages[1].text_content().contains("read:b.txt"));
}

#[tokio::test]
async fn regression_no_replan_when_assistant_reports_success_after_tool_failure() {
    let first_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
        id: "call_1".to_string(),
        name: "read".to_string(),
        arguments: serde_json::json!({}),
    }]);
    let second_assistant = Message::assistant_text("done");
    let client = Arc::new(CapturingMockClient {
        responses: AsyncMutex::new(VecDeque::from([
            ChatResponse {
                message: first_assistant,
                finish_reason: Some("tool_calls".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: second_assistant,
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
        ])),
        requests: AsyncMutex::new(Vec::new()),
    });
    let mut agent = Agent::new(
        client.clone(),
        AgentConfig {
            react_max_replans_on_tool_failure: 1,
            ..AgentConfig::default()
        },
    );
    agent.register_tool(ReadTool);
    let replan_count = Arc::new(AtomicUsize::new(0));
    let replan_count_sink = replan_count.clone();
    agent.subscribe(move |event| {
        if matches!(event, AgentEvent::ReplanTriggered { .. }) {
            replan_count_sink.fetch_add(1, Ordering::Relaxed);
        }
    });

    let messages = agent
        .prompt("read")
        .await
        .expect("prompt should complete without forced replan");
    assert_eq!(
        messages.last().expect("assistant response").text_content(),
        "done"
    );
    assert_eq!(replan_count.load(Ordering::Relaxed), 0);
    assert_eq!(client.requests.lock().await.len(), 2);
}

#[tokio::test]
async fn regression_memory_recall_disabled_when_limit_is_zero() {
    let client = Arc::new(CapturingMockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: Message::assistant_text("ok"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }])),
        requests: AsyncMutex::new(Vec::new()),
    });
    let mut agent = Agent::new(
        client.clone(),
        AgentConfig {
            max_context_messages: Some(2),
            memory_retrieval_limit: 0,
            ..AgentConfig::default()
        },
    );
    agent.append_message(Message::user("postgres connection issue"));
    agent.append_message(Message::assistant_text("ack"));
    agent.append_message(Message::user("retry strategy"));

    let _ = agent.prompt("postgres retry policy").await.expect("prompt");
    let requests = client.requests.lock().await;
    let first_request = requests.first().expect("request should be captured");
    assert!(first_request
        .messages
        .iter()
        .all(|message| !message.text_content().starts_with(MEMORY_RECALL_PREFIX)));
}

#[tokio::test]
async fn functional_context_window_limits_request_messages_and_compacts_history() {
    let client = Arc::new(CapturingMockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: Message::assistant_text("ok"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }])),
        requests: AsyncMutex::new(Vec::new()),
    });

    let mut agent = Agent::new(
        client.clone(),
        AgentConfig {
            max_context_messages: Some(4),
            ..AgentConfig::default()
        },
    );
    agent.append_message(Message::user("u1"));
    agent.append_message(Message::assistant_text("a1"));
    agent.append_message(Message::user("u2"));
    agent.append_message(Message::assistant_text("a2"));
    agent.append_message(Message::user("u3"));

    let _ = agent.prompt("latest").await.expect("prompt should succeed");

    let requests = client.requests.lock().await;
    let first_request = requests.first().expect("request should be captured");
    assert_eq!(first_request.messages.len(), 4);
    assert_eq!(first_request.messages[0].role, MessageRole::System);
    assert_eq!(first_request.messages[1].role, MessageRole::System);
    assert!(first_request.messages[1]
        .text_content()
        .starts_with(CONTEXT_SUMMARY_PREFIX));
    assert_eq!(first_request.messages[2].text_content(), "u3");
    assert_eq!(first_request.messages[3].text_content(), "latest");
    assert!(
        agent.messages().len() <= 4,
        "history should be compacted to configured context window"
    );
}

#[test]
fn unit_bounded_messages_inserts_summary_with_system_prompt() {
    let messages = vec![
        Message::system("sys"),
        Message::user("u1"),
        Message::assistant_text("a1"),
        Message::user("u2"),
        Message::assistant_text("a2"),
    ];

    let bounded = bounded_messages(&messages, 4);
    assert_eq!(bounded.len(), 4);
    assert_eq!(bounded[0].role, MessageRole::System);
    assert_eq!(bounded[1].role, MessageRole::System);
    assert!(bounded[1]
        .text_content()
        .starts_with(CONTEXT_SUMMARY_PREFIX));
    assert_eq!(bounded[2].text_content(), "u2");
    assert_eq!(bounded[3].text_content(), "a2");
}

#[test]
fn regression_bounded_messages_inserts_summary_without_system_prompt() {
    let messages = vec![
        Message::user("u1"),
        Message::assistant_text("a1"),
        Message::user("u2"),
        Message::assistant_text("a2"),
    ];

    let bounded = bounded_messages(&messages, 3);
    assert_eq!(bounded.len(), 3);
    assert_eq!(bounded[0].role, MessageRole::System);
    assert!(bounded[0]
        .text_content()
        .starts_with(CONTEXT_SUMMARY_PREFIX));
    assert_eq!(bounded[1].text_content(), "u2");
    assert_eq!(bounded[2].text_content(), "a2");
}

#[test]
fn regression_truncate_chars_preserves_utf8_and_appends_ellipsis() {
    let long = "alpha beta gamma delta epsilon zeta eta theta";
    let truncated = truncate_chars(long, 12);
    assert_eq!(truncated.chars().count(), 12);
    assert!(truncated.ends_with('…'));

    let long_unicode = "hello 👋 from τau runtime";
    let truncated_unicode = truncate_chars(long_unicode, 9);
    assert_eq!(truncated_unicode.chars().count(), 9);
    assert!(truncated_unicode.ends_with('…'));

    let very_long = "x".repeat(CONTEXT_SUMMARY_MAX_CHARS + 200);
    let clipped = truncate_chars(&very_long, CONTEXT_SUMMARY_MAX_CHARS);
    assert!(clipped.chars().count() <= CONTEXT_SUMMARY_MAX_CHARS);
}

#[tokio::test]
async fn regression_tool_panic_isolated_to_error_tool_result() {
    let first_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
        id: "call_1".to_string(),
        name: "panic_tool".to_string(),
        arguments: serde_json::json!({}),
    }]);
    let second_assistant = Message::assistant_text("continued");
    let client = Arc::new(MockClient {
        responses: AsyncMutex::new(VecDeque::from([
            ChatResponse {
                message: first_assistant,
                finish_reason: Some("tool_calls".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: second_assistant,
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
        ])),
    });

    let mut agent = Agent::new(client, AgentConfig::default());
    agent.register_tool(PanicTool);

    let messages = agent.prompt("panic").await.expect("prompt should continue");
    let tool_message = messages
        .iter()
        .find(|message| message.role == MessageRole::Tool)
        .expect("tool result should be present");
    assert!(tool_message.is_error);
    assert!(tool_message
        .text_content()
        .contains("execution task failed"));
    assert_eq!(
        messages.last().expect("assistant response").text_content(),
        "continued"
    );
}

#[tokio::test]
async fn unit_agent_fork_clones_state_without_aliasing_messages() {
    let first_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
        id: "call_1".to_string(),
        name: "read".to_string(),
        arguments: serde_json::json!({ "path": "README.md" }),
    }]);
    let second_assistant = Message::assistant_text("done");
    let client = Arc::new(MockClient {
        responses: AsyncMutex::new(VecDeque::from([
            ChatResponse {
                message: first_assistant,
                finish_reason: Some("tool_calls".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: second_assistant,
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
        ])),
    });

    let mut base = Agent::new(client, AgentConfig::default());
    base.register_tool(ReadTool);
    base.append_message(Message::user("seed message"));

    let mut fork = base.fork();
    let fork_messages = fork.prompt("read").await.expect("fork prompt");
    assert!(
        fork_messages
            .iter()
            .any(|message| message.role == MessageRole::Tool),
        "fork should inherit registered tools and execute tool calls"
    );
    assert_eq!(base.messages().len(), 2);
    assert_eq!(fork.messages().len(), 6);
}

#[tokio::test]
async fn integration_run_parallel_prompts_executes_runs_concurrently_with_ordered_results() {
    let agent = Agent::new(
        Arc::new(DelayedEchoClient { delay_ms: 90 }),
        AgentConfig::default(),
    );

    let started = Instant::now();
    let results = agent
        .run_parallel_prompts(vec!["prompt-1", "prompt-2", "prompt-3", "prompt-4"], 4)
        .await;
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_millis(260),
        "expected concurrent runs under 260ms, got {elapsed:?}"
    );
    assert_eq!(results.len(), 4);

    for (index, result) in results.into_iter().enumerate() {
        let messages = result.expect("parallel run should succeed");
        assert_eq!(messages[0].role, MessageRole::User);
        assert_eq!(
            messages.last().expect("assistant reply").text_content(),
            format!("echo:prompt-{}", index + 1)
        );
    }
}

#[tokio::test]
async fn integration_bug_6_run_parallel_prompts_allows_zero_parallel_limit() {
    let agent = Agent::new(Arc::new(EchoClient), AgentConfig::default());
    let results = agent.run_parallel_prompts(vec!["p1", "p2", "p3"], 0).await;
    assert_eq!(results.len(), 3);
    for (index, result) in results.into_iter().enumerate() {
        let messages = result.expect("zero parallel limit should clamp to a valid value");
        assert_eq!(
            messages.last().expect("assistant reply").text_content(),
            format!("echo:p{}", index + 1)
        );
    }
}

#[tokio::test]
async fn regression_run_parallel_prompts_isolates_failures_per_prompt() {
    let agent = Agent::new(
        Arc::new(SelectiveFailureEchoClient),
        AgentConfig {
            request_max_retries: 0,
            ..AgentConfig::default()
        },
    );

    let results = agent
        .run_parallel_prompts(vec!["ok-1", "fail-2", "ok-3"], 2)
        .await;

    assert_eq!(results.len(), 3);
    assert!(results[0].as_ref().is_ok());
    assert!(matches!(
        results[1],
        Err(AgentError::Ai(tau_ai::TauAiError::HttpStatus {
            status: 503,
            ..
        }))
    ));
    assert!(results[2].as_ref().is_ok());
}

#[tokio::test]
async fn functional_run_parallel_prompts_returns_empty_for_empty_input() {
    let agent = Agent::new(Arc::new(EchoClient), AgentConfig::default());
    let results = agent
        .run_parallel_prompts(std::iter::empty::<&str>(), 4)
        .await;
    assert!(results.is_empty());
}

#[tokio::test]
async fn functional_memory_backend_persists_entries_and_recalls_across_sessions() {
    let temp = tempfile::tempdir().expect("tempdir");
    let memory_state_dir = temp.path().join("memory-state");
    let config = AgentConfig {
        max_context_messages: Some(2),
        memory_retrieval_limit: 2,
        memory_min_similarity: 0.0,
        memory_backend_state_dir: Some(memory_state_dir.clone()),
        memory_backend_workspace_id: "workspace-a".to_string(),
        memory_backend_max_entries: 100,
        ..AgentConfig::default()
    };

    let mut writer = Agent::new(
        Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([ChatResponse {
                message: Message::assistant_text("stored"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            }])),
        }),
        config.clone(),
    );
    writer
        .prompt("tokio retry checklist for release rollback")
        .await
        .expect("writer prompt should succeed");

    let backend_file = memory_state_dir
        .join("live-backend")
        .join("workspace-a.jsonl");
    assert!(backend_file.exists(), "expected persisted backend file");
    let raw_backend = std::fs::read_to_string(&backend_file).expect("read backend file");
    assert!(raw_backend.contains("tokio retry checklist"));

    let reader_client = Arc::new(CapturingMockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: Message::assistant_text("ack"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }])),
        requests: AsyncMutex::new(Vec::new()),
    });
    let mut reader = Agent::new(reader_client.clone(), config);
    reader
        .prompt("what is our rollback checklist?")
        .await
        .expect("reader prompt should succeed");

    let requests = reader_client.requests.lock().await;
    let request = requests.first().expect("one request");
    let memory_recall = request
        .messages
        .iter()
        .find(|message| {
            message.role == MessageRole::System
                && message.text_content().contains(MEMORY_RECALL_PREFIX)
        })
        .expect("memory recall system message");
    assert!(memory_recall.text_content().contains("tokio"));
    assert!(memory_recall.text_content().contains("rollback"));
}

#[tokio::test]
async fn integration_memory_backend_recall_orders_relevant_entries_for_multiturn_topics() {
    let temp = tempfile::tempdir().expect("tempdir");
    let memory_state_dir = temp.path().join("memory-state");
    let config = AgentConfig {
        max_context_messages: Some(2),
        memory_retrieval_limit: 2,
        memory_min_similarity: 0.0,
        memory_backend_state_dir: Some(memory_state_dir.clone()),
        memory_backend_workspace_id: "workspace-quality".to_string(),
        memory_backend_max_entries: 128,
        ..AgentConfig::default()
    };

    let mut writer = Agent::new(
        Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([
                ChatResponse {
                    message: Message::assistant_text("postgres failover uses promote + lag checks"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
                ChatResponse {
                    message: Message::assistant_text("redis warmup uses hot-key preload"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
                ChatResponse {
                    message: Message::assistant_text("kafka lag response uses rebalance"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
            ])),
        }),
        config.clone(),
    );
    writer
        .prompt("postgres failover checklist with replication lag verification")
        .await
        .expect("writer prompt postgres");
    writer
        .prompt("redis warmup checklist with key preload")
        .await
        .expect("writer prompt redis");
    writer
        .prompt("kafka lag remediation for consumer backlog")
        .await
        .expect("writer prompt kafka");

    let reader_client = Arc::new(CapturingMockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: Message::assistant_text("ack"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }])),
        requests: AsyncMutex::new(Vec::new()),
    });
    let mut reader = Agent::new(reader_client.clone(), config);
    reader
        .prompt("how do we handle postgres failover lag checks?")
        .await
        .expect("reader prompt should succeed");

    let requests = reader_client.requests.lock().await;
    let request = requests.first().expect("captured request");
    let memory_recall = request
        .messages
        .iter()
        .find(|message| {
            message.role == MessageRole::System
                && message.text_content().contains(MEMORY_RECALL_PREFIX)
        })
        .expect("memory recall system message");
    let recall_text = memory_recall.text_content();
    let recall_lines = recall_text.lines().skip(1).collect::<Vec<_>>();
    assert_eq!(recall_lines.len(), 2, "expected top-2 recall entries");
    assert!(
        recall_lines[0].contains("postgres"),
        "top recall item should prioritize postgres context: {}",
        recall_lines[0]
    );
    assert!(
        !recall_lines[0].contains("redis warmup"),
        "top recall item should not rank redis warmup first for postgres query"
    );
}

#[tokio::test]
async fn integration_memory_backend_unavailable_falls_back_to_context_history_recall() {
    let temp = tempfile::tempdir().expect("tempdir");
    let memory_state_path = temp.path().join("memory-state-as-file");
    std::fs::write(&memory_state_path, "not-a-directory").expect("write backend blocker file");

    let reader_client = Arc::new(CapturingMockClient {
        responses: AsyncMutex::new(VecDeque::from([
            ChatResponse {
                message: Message::assistant_text("stored"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: Message::assistant_text("recalled"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
        ])),
        requests: AsyncMutex::new(Vec::new()),
    });
    let mut agent = Agent::new(
        reader_client.clone(),
        AgentConfig {
            system_prompt: String::new(),
            max_context_messages: Some(2),
            memory_retrieval_limit: 1,
            memory_min_similarity: 0.0,
            response_cache_enabled: false,
            memory_backend_state_dir: Some(memory_state_path.clone()),
            memory_backend_workspace_id: "ops".to_string(),
            ..AgentConfig::default()
        },
    );

    agent
        .prompt("tokio rollback checklist")
        .await
        .expect("first prompt");
    agent
        .prompt("tokio rollback checklist")
        .await
        .expect("second prompt");

    let requests = reader_client.requests.lock().await;
    assert_eq!(requests.len(), 2);
    let second = &requests[1];
    let memory_recall = second
        .messages
        .iter()
        .find(|message| {
            message.role == MessageRole::System
                && message.text_content().contains(MEMORY_RECALL_PREFIX)
        })
        .expect("history-backed recall system message");
    assert!(memory_recall
        .text_content()
        .contains("tokio rollback checklist"));
    assert!(
        !memory_state_path.join("live-backend").exists(),
        "backend path should stay disabled when state dir is not usable"
    );
}

#[tokio::test]
async fn regression_memory_backend_respects_max_entries_cap() {
    let temp = tempfile::tempdir().expect("tempdir");
    let memory_state_dir = temp.path().join("memory-state");
    let config = AgentConfig {
        memory_retrieval_limit: 1,
        memory_min_similarity: 0.0,
        memory_backend_state_dir: Some(memory_state_dir.clone()),
        memory_backend_workspace_id: "workspace-cap".to_string(),
        memory_backend_max_entries: 2,
        ..AgentConfig::default()
    };

    let mut writer = Agent::new(
        Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([
                ChatResponse {
                    message: Message::assistant_text("postgres ack"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
                ChatResponse {
                    message: Message::assistant_text("redis ack"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
                ChatResponse {
                    message: Message::assistant_text("kafka ack"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
            ])),
        }),
        config,
    );

    writer
        .prompt("postgres failover prompt")
        .await
        .expect("prompt postgres");
    writer
        .prompt("redis warmup prompt")
        .await
        .expect("prompt redis");
    writer
        .prompt("kafka lag prompt")
        .await
        .expect("prompt kafka");

    let backend_file = memory_state_dir
        .join("live-backend")
        .join("workspace-cap.jsonl");
    let persisted_lines = std::fs::read_to_string(&backend_file)
        .expect("read persisted backend")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    assert_eq!(
        persisted_lines.len(),
        2,
        "backend file should keep only the most recent capped entries"
    );
    let persisted_joined = persisted_lines.join("\n");
    assert!(
        persisted_joined.contains("kafka lag prompt"),
        "newest user prompt should remain after cap"
    );
    assert!(
        persisted_joined.contains("kafka ack"),
        "newest assistant response should remain after cap"
    );
    assert!(
        !persisted_joined.contains("postgres failover prompt"),
        "oldest entries should be evicted when max_entries is reached"
    );
}

#[tokio::test]
async fn regression_memory_backend_recall_falls_back_to_hash_when_embedding_api_fails() {
    let server = MockServer::start();
    let embedding_mock = server.mock(|when, then| {
        when.method(POST).path("/embeddings");
        then.status(500);
    });

    let temp = tempfile::tempdir().expect("tempdir");
    let memory_state_dir = temp.path().join("memory-state");
    let config = AgentConfig {
        max_context_messages: Some(2),
        memory_retrieval_limit: 1,
        memory_min_similarity: 0.0,
        memory_embedding_dimensions: 64,
        memory_embedding_model: Some("text-embedding-3-small".to_string()),
        memory_embedding_api_base: Some(server.url("")),
        memory_embedding_api_key: Some("test-key".to_string()),
        memory_backend_state_dir: Some(memory_state_dir.clone()),
        memory_backend_workspace_id: "default".to_string(),
        ..AgentConfig::default()
    };

    let mut writer = Agent::new(
        Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([ChatResponse {
                message: Message::assistant_text("stored"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            }])),
        }),
        config.clone(),
    );
    writer
        .prompt("tokio runtime troubleshooting checklist")
        .await
        .expect("writer prompt");

    let reader_client = Arc::new(CapturingMockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: Message::assistant_text("ack"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }])),
        requests: AsyncMutex::new(Vec::new()),
    });
    let mut reader = Agent::new(reader_client.clone(), config);
    reader
        .prompt("tokio runtime?")
        .await
        .expect("reader prompt");

    embedding_mock.assert();
    let requests = reader_client.requests.lock().await;
    let request = requests.first().expect("one request");
    let memory_recall = request
        .messages
        .iter()
        .find(|message| {
            message.role == MessageRole::System
                && message.text_content().contains(MEMORY_RECALL_PREFIX)
        })
        .expect("memory recall system message");
    assert!(memory_recall.text_content().contains("tokio runtime"));
}
