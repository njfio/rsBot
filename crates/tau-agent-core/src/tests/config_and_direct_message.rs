use super::*;

#[test]
fn unit_cache_insert_with_limit_evicts_oldest_entries() {
    let mut cache = HashMap::new();
    let mut order = VecDeque::new();
    cache_insert_with_limit(&mut cache, &mut order, "a".to_string(), 1_u32, 2);
    cache_insert_with_limit(&mut cache, &mut order, "b".to_string(), 2_u32, 2);
    cache_insert_with_limit(&mut cache, &mut order, "c".to_string(), 3_u32, 2);

    assert_eq!(cache.len(), 2);
    assert!(!cache.contains_key("a"));
    assert_eq!(cache.get("b"), Some(&2));
    assert_eq!(cache.get("c"), Some(&3));
}

#[test]
fn unit_dynamic_tool_registry_supports_presence_and_lifecycle_helpers() {
    let mut agent = Agent::new(Arc::new(EchoClient), AgentConfig::default());
    assert!(!agent.has_tool("read"));
    assert!(!agent.unregister_tool("read"));

    agent.register_tool(ReadTool);
    assert!(agent.has_tool("read"));
    assert_eq!(agent.registered_tool_names(), vec!["read".to_string()]);

    assert!(agent.unregister_tool("read"));
    assert!(!agent.has_tool("read"));

    agent.register_tool(ReadTool);
    agent.clear_tools();
    assert!(agent.registered_tool_names().is_empty());
}

#[test]
fn unit_direct_message_policy_enforces_configured_routes() {
    let mut policy = AgentDirectMessagePolicy::default();
    assert!(!policy.allows("planner", "executor"));
    assert!(!policy.allows("planner", "planner"));

    policy.allow_route("planner", "executor");
    assert!(policy.allows("planner", "executor"));
    assert!(!policy.allows("executor", "planner"));

    policy.allow_bidirectional_route("reviewer", "executor");
    assert!(policy.allows("reviewer", "executor"));
    assert!(policy.allows("executor", "reviewer"));

    policy.allow_self_messages = true;
    assert!(policy.allows("planner", "planner"));
}

#[test]
fn functional_send_direct_message_appends_system_message() {
    let sender = Agent::new(
        Arc::new(EchoClient),
        AgentConfig {
            agent_id: "planner".to_string(),
            ..AgentConfig::default()
        },
    );
    let mut recipient = Agent::new(
        Arc::new(EchoClient),
        AgentConfig {
            agent_id: "executor".to_string(),
            ..AgentConfig::default()
        },
    );
    let mut policy = AgentDirectMessagePolicy::default();
    policy.allow_route("planner", "executor");

    sender
        .send_direct_message(&mut recipient, "  review this step  ", &policy)
        .expect("direct message should be accepted");

    let direct_message = recipient
        .messages()
        .iter()
        .find(|message| {
            message.role == MessageRole::System
                && message.text_content().starts_with(DIRECT_MESSAGE_PREFIX)
        })
        .expect("direct message should be appended as a system message");
    assert!(direct_message
        .text_content()
        .contains("from=planner to=executor"));
    assert!(direct_message.text_content().contains("review this step"));
}

#[tokio::test]
async fn integration_direct_message_is_included_in_recipient_prompt_context() {
    let sender = Agent::new(
        Arc::new(EchoClient),
        AgentConfig {
            agent_id: "planner".to_string(),
            ..AgentConfig::default()
        },
    );
    let recipient_client = Arc::new(CapturingMockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: Message::assistant_text("ack"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }])),
        requests: AsyncMutex::new(Vec::new()),
    });
    let mut recipient = Agent::new(
        recipient_client.clone(),
        AgentConfig {
            agent_id: "executor".to_string(),
            ..AgentConfig::default()
        },
    );
    let mut policy = AgentDirectMessagePolicy::default();
    policy.allow_route("planner", "executor");

    sender
        .send_direct_message(&mut recipient, "Focus on retry semantics", &policy)
        .expect("route should be authorized");
    let _ = recipient
        .prompt("continue")
        .await
        .expect("recipient prompt should succeed");

    let requests = recipient_client.requests.lock().await;
    let request = requests.first().expect("captured request");
    assert!(
        request.messages.iter().any(|message| {
            message.role == MessageRole::System
                && message
                    .text_content()
                    .contains("[Tau direct message] from=planner to=executor")
        }),
        "direct message should be included in prompt context"
    );
}

#[test]
fn regression_unauthorized_direct_message_fails_closed_without_mutation() {
    let sender = Agent::new(
        Arc::new(EchoClient),
        AgentConfig {
            agent_id: "planner".to_string(),
            ..AgentConfig::default()
        },
    );
    let mut recipient = Agent::new(
        Arc::new(EchoClient),
        AgentConfig {
            agent_id: "executor".to_string(),
            ..AgentConfig::default()
        },
    );
    let policy = AgentDirectMessagePolicy::default();
    let baseline_count = recipient.messages().len();

    let error = sender
        .send_direct_message(&mut recipient, "unauthorized", &policy)
        .expect_err("unauthorized route must fail closed");
    assert!(matches!(
        error,
        AgentDirectMessageError::UnauthorizedRoute { .. }
    ));
    assert_eq!(recipient.messages().len(), baseline_count);
}

#[test]
fn regression_direct_message_policy_enforces_max_message_chars() {
    let sender = Agent::new(
        Arc::new(EchoClient),
        AgentConfig {
            agent_id: "planner".to_string(),
            ..AgentConfig::default()
        },
    );
    let mut recipient = Agent::new(
        Arc::new(EchoClient),
        AgentConfig {
            agent_id: "executor".to_string(),
            ..AgentConfig::default()
        },
    );
    let mut policy = AgentDirectMessagePolicy::default();
    policy.allow_route("planner", "executor");
    policy.max_message_chars = 5;
    let baseline_count = recipient.messages().len();

    let error = sender
        .send_direct_message(&mut recipient, "message too long", &policy)
        .expect_err("oversized direct message must fail");
    assert!(matches!(
        error,
        AgentDirectMessageError::MessageTooLong { .. }
    ));
    assert_eq!(recipient.messages().len(), baseline_count);
}

#[tokio::test]
async fn functional_with_scoped_tool_registers_within_scope_and_restores_after() {
    let mut agent = Agent::new(Arc::new(EchoClient), AgentConfig::default());
    assert!(!agent.has_tool("read"));

    let value = agent
        .with_scoped_tool(ReadTool, |agent| {
            Box::pin(async move {
                assert!(agent.has_tool("read"));
                assert_eq!(agent.registered_tool_names(), vec!["read".to_string()]);
                42usize
            })
        })
        .await;

    assert_eq!(value, 42);
    assert!(!agent.has_tool("read"));
}

#[tokio::test]
async fn spec_c05_swap_dispatch_model_overrides_dispatch_and_restores_baseline() {
    let client = Arc::new(CapturingMockClient {
        responses: AsyncMutex::new(VecDeque::from([
            ChatResponse {
                message: Message::assistant_text("inside"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: Message::assistant_text("outside"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
        ])),
        requests: AsyncMutex::new(Vec::new()),
    });
    let mut agent = Agent::new(
        client.clone(),
        AgentConfig {
            model: "openai/gpt-4.1-mini".to_string(),
            ..AgentConfig::default()
        },
    );

    let previous_model = agent.swap_dispatch_model("openai/gpt-5.2");
    let _ = agent
        .prompt("inside")
        .await
        .expect("inside prompt should succeed");
    agent.restore_dispatch_model(previous_model);
    let _ = agent
        .prompt("outside")
        .await
        .expect("outside prompt should succeed");

    let requests = client.requests.lock().await;
    assert_eq!(requests[0].model, "openai/gpt-5.2");
    assert_eq!(requests[1].model, "openai/gpt-4.1-mini");
}

#[tokio::test]
async fn unit_cooperative_cancellation_token_signals_waiters() {
    let token = CooperativeCancellationToken::new();
    let waiter = token.clone();
    let task = tokio::spawn(async move {
        waiter.cancelled().await;
        1usize
    });

    tokio::time::sleep(Duration::from_millis(5)).await;
    token.cancel();

    assert!(token.is_cancelled());
    assert_eq!(task.await.expect("waiter task should complete"), 1);
}

#[test]
fn unit_estimate_chat_request_tokens_accounts_for_tools_and_max_tokens() {
    let request = ChatRequest {
        model: "openai/gpt-4o-mini".to_string(),
        messages: vec![
            Message::system("sys"),
            Message::user("hello world"),
            Message::assistant_blocks(vec![ContentBlock::ToolCall {
                id: "call-1".to_string(),
                name: "read".to_string(),
                arguments: serde_json::json!({ "path": "README.md" }),
            }]),
        ],
        tools: vec![ToolDefinition {
            name: "read".to_string(),
            description: "Read file contents".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                }
            }),
        }],
        tool_choice: Some(ToolChoice::Auto),
        json_mode: false,
        max_tokens: Some(64),
        temperature: Some(0.0),
        prompt_cache: Default::default(),
    };

    let estimate = estimate_chat_request_tokens(&request);
    assert!(estimate.input_tokens > 0);
    assert_eq!(
        estimate.total_tokens,
        estimate.input_tokens.saturating_add(64)
    );
}

#[test]
fn functional_estimate_chat_request_tokens_accounts_for_media_blocks() {
    let baseline = ChatRequest {
        model: "openai/gpt-4o-mini".to_string(),
        messages: vec![Message::user("hello")],
        tools: vec![],
        tool_choice: None,
        json_mode: false,
        max_tokens: Some(32),
        temperature: None,
        prompt_cache: Default::default(),
    };
    let with_media = ChatRequest {
        model: "openai/gpt-4o-mini".to_string(),
        messages: vec![Message {
            role: MessageRole::User,
            content: vec![
                ContentBlock::text("hello"),
                ContentBlock::image_base64("image/png", "aW1hZ2VEYXRh"),
                ContentBlock::audio_base64("audio/wav", "YXVkaW9EYXRh"),
            ],
            tool_call_id: None,
            tool_name: None,
            is_error: false,
        }],
        tools: vec![],
        tool_choice: None,
        json_mode: false,
        max_tokens: Some(32),
        temperature: None,
        prompt_cache: Default::default(),
    };

    let baseline_estimate = estimate_chat_request_tokens(&baseline);
    let media_estimate = estimate_chat_request_tokens(&with_media);
    assert!(media_estimate.input_tokens > baseline_estimate.input_tokens);
    assert!(media_estimate.total_tokens > baseline_estimate.total_tokens);
}

#[tokio::test]
async fn prompt_without_tools_completes_in_one_turn() {
    let client = Arc::new(MockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: Message::assistant_text("Hello from model"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }])),
    });

    let mut agent = Agent::new(client, AgentConfig::default());
    let new_messages = agent.prompt("hi").await.expect("prompt should succeed");

    assert_eq!(new_messages.len(), 2);
    assert_eq!(new_messages[0].role, MessageRole::User);
    assert_eq!(new_messages[1].text_content(), "Hello from model");
}

#[tokio::test]
async fn functional_response_cache_reuses_model_response_for_identical_request() {
    let calls = Arc::new(AtomicUsize::new(0));
    let client = Arc::new(CountingStaticClient {
        calls: calls.clone(),
        response: ChatResponse {
            message: Message::assistant_text("cached"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        },
    });

    let mut agent = Agent::new(client, AgentConfig::default());
    agent.append_message(Message::user("cache me"));
    let baseline_messages = agent.messages.clone();
    let start_index = baseline_messages.len().saturating_sub(1);

    let _ = agent
        .run_loop(start_index, None, false)
        .await
        .expect("first run should succeed");
    agent.messages = baseline_messages;
    let _ = agent
        .run_loop(start_index, None, false)
        .await
        .expect("second run should succeed");

    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn regression_response_cache_disabled_dispatches_each_time() {
    let calls = Arc::new(AtomicUsize::new(0));
    let client = Arc::new(CountingStaticClient {
        calls: calls.clone(),
        response: ChatResponse {
            message: Message::assistant_text("uncached"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        },
    });
    let mut agent = Agent::new(
        client,
        AgentConfig {
            response_cache_enabled: false,
            ..AgentConfig::default()
        },
    );
    agent.append_message(Message::user("cache disabled"));
    let baseline_messages = agent.messages.clone();
    let start_index = baseline_messages.len().saturating_sub(1);

    let _ = agent
        .run_loop(start_index, None, false)
        .await
        .expect("first run should succeed");
    agent.messages = baseline_messages;
    let _ = agent
        .run_loop(start_index, None, false)
        .await
        .expect("second run should succeed");

    assert_eq!(calls.load(Ordering::Relaxed), 2);
}

#[tokio::test]
async fn prompt_executes_tool_calls_and_continues() {
    let first_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
        id: "call_1".to_string(),
        name: "read".to_string(),
        arguments: serde_json::json!({ "path": "README.md" }),
    }]);

    let second_assistant = Message::assistant_text("Done reading file");

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
    agent.register_tool(ReadTool);

    let new_messages = agent
        .prompt("Read README.md")
        .await
        .expect("prompt should succeed");

    assert_eq!(new_messages.len(), 4);
    assert_eq!(new_messages[0].role, MessageRole::User);
    assert_eq!(new_messages[1].role, MessageRole::Assistant);
    assert_eq!(new_messages[2].role, MessageRole::Tool);
    assert!(new_messages[2].text_content().contains("read:README.md"));
    assert_eq!(new_messages[3].text_content(), "Done reading file");
}

#[tokio::test]
async fn integration_spec_c03_prompt_skip_tool_call_terminates_run_without_follow_up_model_turn() {
    struct SkipDirectiveTool;

    #[async_trait]
    impl AgentTool for SkipDirectiveTool {
        fn definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: "skip".to_string(),
                description: "Suppress outbound response for this turn".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "reason": { "type": "string" }
                    },
                    "additionalProperties": false
                }),
            }
        }

        async fn execute(&self, arguments: serde_json::Value) -> ToolExecutionResult {
            let reason = arguments
                .get("reason")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            ToolExecutionResult::ok(serde_json::json!({
                "skip_response": true,
                "reason": reason,
                "reason_code": "skip_suppressed"
            }))
        }
    }

    let first_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
        id: "call_skip_1".to_string(),
        name: "skip".to_string(),
        arguments: serde_json::json!({ "reason": "already acknowledged" }),
    }]);

    let client = Arc::new(MockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: first_assistant,
            finish_reason: Some("tool_calls".to_string()),
            usage: ChatUsage::default(),
        }])),
    });

    let mut agent = Agent::new(client, AgentConfig::default());
    agent.register_tool(SkipDirectiveTool);

    let new_messages = agent
        .prompt("ack")
        .await
        .expect("skip directive should terminate turn without second model call");

    assert_eq!(new_messages.len(), 3);
    assert_eq!(new_messages[0].role, MessageRole::User);
    assert_eq!(new_messages[1].role, MessageRole::Assistant);
    assert_eq!(new_messages[2].role, MessageRole::Tool);
    assert_eq!(new_messages[2].tool_name.as_deref(), Some("skip"));
}

#[test]
fn spec_c04_extract_skip_response_reason_detects_valid_skip_tool_payload() {
    let messages = vec![Message::tool_result(
        "call_skip_1",
        "skip",
        r#"{"skip_response":true,"reason":"duplicate response","reason_code":"skip_suppressed"}"#,
        false,
    )];
    let reason = crate::extract_skip_response_reason(&messages);
    assert_eq!(reason.as_deref(), Some("duplicate response"));
}

#[tokio::test]
async fn integration_spec_2520_c03_prompt_react_tool_call_terminates_run_without_follow_up_model_turn(
) {
    struct ReactDirectiveTool;

    #[async_trait]
    impl AgentTool for ReactDirectiveTool {
        fn definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: "react".to_string(),
                description: "Dispatch a reaction without sending a textual reply".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "emoji": { "type": "string" },
                        "message_id": { "type": "string" }
                    },
                    "required": ["emoji"],
                    "additionalProperties": false
                }),
            }
        }

        async fn execute(&self, arguments: serde_json::Value) -> ToolExecutionResult {
            let emoji = arguments
                .get("emoji")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let message_id = arguments
                .get("message_id")
                .and_then(serde_json::Value::as_str);
            ToolExecutionResult::ok(serde_json::json!({
                "react_response": true,
                "emoji": emoji,
                "message_id": message_id,
                "reason_code": "react_requested",
                "suppress_response": true
            }))
        }
    }

    let first_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
        id: "call_react_1".to_string(),
        name: "react".to_string(),
        arguments: serde_json::json!({
            "emoji": "👍",
            "message_id": "42"
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
    agent.register_tool(ReactDirectiveTool);

    let new_messages = agent
        .prompt("ack with reaction only")
        .await
        .expect("react directive should terminate turn without second model call");

    assert_eq!(new_messages.len(), 3);
    assert_eq!(new_messages[0].role, MessageRole::User);
    assert_eq!(new_messages[1].role, MessageRole::Assistant);
    assert_eq!(new_messages[2].role, MessageRole::Tool);
    assert_eq!(new_messages[2].tool_name.as_deref(), Some("react"));
}

#[test]
fn spec_2520_c04_extract_reaction_request_detects_valid_react_tool_payload() {
    let messages = vec![Message::tool_result(
        "call_react_1",
        "react",
        r#"{"react_response":true,"emoji":"👍","message_id":"42","reason_code":"react_requested","suppress_response":true}"#,
        false,
    )];
    let directive = crate::extract_react_response_directive(&messages)
        .expect("expected valid reaction directive");
    assert_eq!(directive.emoji, "👍");
    assert_eq!(directive.message_id.as_deref(), Some("42"));
}

#[test]
fn spec_2520_c04_extract_reaction_request_accepts_action_only_payload() {
    let messages = vec![Message::tool_result(
        "call_react_2",
        "react",
        r#"{"action":"react_response","emoji":"✅","suppress_response":true}"#,
        false,
    )];
    let directive = crate::extract_react_response_directive(&messages)
        .expect("expected action-based reaction directive");
    assert_eq!(directive.emoji, "✅");
    assert_eq!(directive.message_id, None);
    assert_eq!(directive.reason_code, "react_requested");
}

#[test]
fn regression_2520_extract_reaction_request_ignores_error_tool_messages() {
    let messages = vec![Message::tool_result(
        "call_react_3",
        "react",
        r#"{"react_response":true,"emoji":"👍","message_id":"42","suppress_response":true}"#,
        true,
    )];
    let directive = crate::extract_react_response_directive(&messages);
    assert!(directive.is_none());
}

#[test]
fn regression_2520_extract_reaction_request_defaults_empty_reason_code() {
    let messages = vec![Message::tool_result(
        "call_react_4",
        "react",
        r#"{"react_response":true,"emoji":"👍","reason_code":"","suppress_response":true}"#,
        false,
    )];
    let directive = crate::extract_react_response_directive(&messages)
        .expect("expected valid reaction directive");
    assert_eq!(directive.reason_code, "react_requested");
}

#[tokio::test]
async fn regression_2520_prompt_react_tool_error_does_not_suppress_follow_up_model_turn() {
    struct ReactErrorTool;

    #[async_trait]
    impl AgentTool for ReactErrorTool {
        fn definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: "react".to_string(),
                description: "returns error for regression coverage".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "emoji": { "type": "string" }
                    },
                    "required": ["emoji"],
                    "additionalProperties": false
                }),
            }
        }

        async fn execute(&self, _arguments: serde_json::Value) -> ToolExecutionResult {
            ToolExecutionResult::error(serde_json::json!({
                "error": "reaction rejected",
                "reason_code": "react_rejected"
            }))
        }
    }

    let first_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
        id: "call_react_err_1".to_string(),
        name: "react".to_string(),
        arguments: serde_json::json!({ "emoji": "👍" }),
    }]);
    let second_assistant = Message::assistant_text("fallback response");
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
    agent.register_tool(ReactErrorTool);

    let new_messages = agent
        .prompt("react if possible")
        .await
        .expect("react error should not suppress fallback model turn");
    assert_eq!(new_messages.len(), 4);
    assert_eq!(new_messages[3].text_content(), "fallback response");
}

#[tokio::test]
async fn regression_2520_prompt_react_tool_without_directive_payload_does_not_suppress_follow_up_model_turn(
) {
    struct ReactMalformedTool;

    #[async_trait]
    impl AgentTool for ReactMalformedTool {
        fn definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: "react".to_string(),
                description: "returns malformed payload for regression coverage".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "emoji": { "type": "string" }
                    },
                    "required": ["emoji"],
                    "additionalProperties": false
                }),
            }
        }

        async fn execute(&self, _arguments: serde_json::Value) -> ToolExecutionResult {
            ToolExecutionResult::ok(serde_json::json!({
                "status": "queued_without_directive_marker"
            }))
        }
    }

    let first_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
        id: "call_react_bad_1".to_string(),
        name: "react".to_string(),
        arguments: serde_json::json!({ "emoji": "👍" }),
    }]);
    let second_assistant = Message::assistant_text("fallback response");
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
    agent.register_tool(ReactMalformedTool);

    let new_messages = agent
        .prompt("react if possible")
        .await
        .expect("malformed react payload should not suppress fallback model turn");
    assert_eq!(new_messages.len(), 4);
    assert_eq!(new_messages[3].text_content(), "fallback response");
}

#[tokio::test]
async fn integration_scoped_tool_lifecycle_supports_prompt_execution() {
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

    let mut agent = Agent::new(client, AgentConfig::default());
    assert!(!agent.has_tool("read"));

    let messages = agent
        .with_scoped_tool(ReadTool, |agent| {
            Box::pin(async move { agent.prompt("read").await })
        })
        .await
        .expect("scoped tool prompt should succeed");

    assert!(
        messages
            .iter()
            .any(|message| message.role == MessageRole::Tool),
        "scoped tool should be available while running the closure"
    );
    assert!(!agent.has_tool("read"));
}

#[tokio::test]
async fn regression_scoped_tool_restores_replaced_tool_and_avoids_stale_cache() {
    let make_tool_call = |id: &str| {
        Message::assistant_blocks(vec![ContentBlock::ToolCall {
            id: id.to_string(),
            name: "cacheable_read".to_string(),
            arguments: serde_json::json!({ "path": "a.txt" }),
        }])
    };
    let client = Arc::new(MockClient {
        responses: AsyncMutex::new(VecDeque::from([
            ChatResponse {
                message: make_tool_call("call_1"),
                finish_reason: Some("tool_calls".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: Message::assistant_text("base pass 1"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: make_tool_call("call_2"),
                finish_reason: Some("tool_calls".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: Message::assistant_text("scoped pass"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: make_tool_call("call_3"),
                finish_reason: Some("tool_calls".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: Message::assistant_text("base pass 2"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
        ])),
    });

    let base_calls = Arc::new(AtomicUsize::new(0));
    let scoped_calls = Arc::new(AtomicUsize::new(0));
    let mut agent = Agent::new(client, AgentConfig::default());
    agent.register_tool(CacheableVariantTool {
        label: "base",
        calls: base_calls.clone(),
    });

    let first = agent
        .prompt("first run")
        .await
        .expect("base tool run should succeed");
    let first_tool = first
        .iter()
        .find(|message| message.role == MessageRole::Tool)
        .expect("first tool result");
    assert!(first_tool.text_content().contains("base:a.txt"));

    let second = agent
        .with_scoped_tool(
            CacheableVariantTool {
                label: "scoped",
                calls: scoped_calls.clone(),
            },
            |agent| Box::pin(async move { agent.prompt("second run").await }),
        )
        .await
        .expect("scoped tool run should succeed");
    let second_tool = second
        .iter()
        .find(|message| message.role == MessageRole::Tool)
        .expect("second tool result");
    assert!(second_tool.text_content().contains("scoped:a.txt"));

    let third = agent
        .prompt("third run")
        .await
        .expect("restored base tool run should succeed");
    let third_tool = third
        .iter()
        .find(|message| message.role == MessageRole::Tool)
        .expect("third tool result");
    assert!(third_tool.text_content().contains("base:a.txt"));

    assert_eq!(base_calls.load(Ordering::Relaxed), 2);
    assert_eq!(scoped_calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn emits_expected_event_sequence_for_tool_turn() {
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

    let mut agent = Agent::new(client, AgentConfig::default());
    agent.register_tool(ReadTool);

    let events = Arc::new(Mutex::new(Vec::<String>::new()));
    let recorded = events.clone();
    agent.subscribe(move |event| {
        let label = match event {
            AgentEvent::MessageAdded { message } => format!("message:{:?}", message.role),
            AgentEvent::ToolExecutionStart { tool_name, .. } => {
                format!("tool_start:{tool_name}")
            }
            AgentEvent::ToolExecutionEnd { tool_name, .. } => format!("tool_end:{tool_name}"),
            AgentEvent::TurnStart { turn } => format!("turn_start:{turn}"),
            AgentEvent::TurnEnd { turn, .. } => format!("turn_end:{turn}"),
            AgentEvent::ReplanTriggered { turn, .. } => format!("replan:{turn}"),
            AgentEvent::CostUpdated { turn, .. } => format!("cost:{turn}"),
            AgentEvent::CostBudgetAlert {
                threshold_percent, ..
            } => format!("cost_alert:{threshold_percent}"),
            AgentEvent::SafetyPolicyApplied { stage, .. } => {
                format!("safety:{}", stage.as_str())
            }
            AgentEvent::AgentStart => "agent_start".to_string(),
            AgentEvent::AgentEnd { .. } => "agent_end".to_string(),
        };

        recorded
            .lock()
            .expect("event mutex should lock")
            .push(label);
    });

    let _ = agent.prompt("read").await.expect("prompt should succeed");

    let events = events.lock().expect("event mutex should lock").clone();
    assert_eq!(
        events,
        vec![
            "message:User",
            "agent_start",
            "turn_start:1",
            "message:Assistant",
            "tool_start:read",
            "tool_end:read",
            "message:Tool",
            "turn_end:1",
            "turn_start:2",
            "message:Assistant",
            "turn_end:2",
            "agent_end",
        ]
    );
}

#[tokio::test]
async fn returns_max_turns_exceeded_for_infinite_tool_loop() {
    let first_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
        id: "call_1".to_string(),
        name: "read".to_string(),
        arguments: serde_json::json!({ "path": "README.md" }),
    }]);
    let second_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
        id: "call_2".to_string(),
        name: "read".to_string(),
        arguments: serde_json::json!({ "path": "README.md" }),
    }]);

    let client = Arc::new(MockClient {
        responses: AsyncMutex::new(VecDeque::from([
            ChatResponse {
                message: first_assistant,
                finish_reason: Some("tool_calls".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: second_assistant,
                finish_reason: Some("tool_calls".to_string()),
                usage: ChatUsage::default(),
            },
        ])),
    });

    let mut agent = Agent::new(
        client,
        AgentConfig {
            max_turns: 2,
            ..AgentConfig::default()
        },
    );
    agent.register_tool(ReadTool);

    let error = agent.prompt("loop").await.expect_err("must hit max turns");
    match error {
        AgentError::MaxTurnsExceeded(2) => {}
        other => panic!("expected AgentError::MaxTurnsExceeded(2), got {other:?}"),
    }
}

#[tokio::test]
async fn rejects_invalid_tool_arguments_via_json_schema() {
    let assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
        id: "call_1".to_string(),
        name: "read".to_string(),
        arguments: serde_json::json!({}),
    }]);

    let final_assistant = Message::assistant_text("done");

    let client = Arc::new(MockClient {
        responses: AsyncMutex::new(VecDeque::from([
            ChatResponse {
                message: assistant,
                finish_reason: Some("tool_calls".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: final_assistant,
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
        ])),
    });

    let mut agent = Agent::new(client, AgentConfig::default());
    agent.register_tool(ReadTool);

    let messages = agent
        .prompt("read without args")
        .await
        .expect("prompt succeeds");
    let tool_message = messages
        .iter()
        .find(|message| message.role == MessageRole::Tool)
        .expect("tool result must exist");
    assert!(tool_message.is_error);
    assert!(tool_message.text_content().contains("invalid arguments"));
}
