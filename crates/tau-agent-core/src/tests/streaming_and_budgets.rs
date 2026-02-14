use super::*;

#[test]
fn unit_agent_config_defaults_include_request_and_tool_timeouts() {
    let config = AgentConfig::default();
    assert_eq!(config.agent_id, "tau-agent");
    assert_eq!(config.request_timeout_ms, Some(120_000));
    assert_eq!(config.tool_timeout_ms, Some(120_000));
    assert!(config.stream_retry_with_buffering);
    assert_eq!(config.max_estimated_input_tokens, Some(120_000));
    assert_eq!(config.max_estimated_total_tokens, None);
    assert_eq!(config.structured_output_max_retries, 1);
    assert_eq!(config.react_max_replans_on_tool_failure, 1);
    assert_eq!(config.memory_retrieval_limit, 3);
    assert_eq!(config.memory_embedding_dimensions, 128);
    assert_eq!(config.memory_min_similarity, 0.55);
    assert_eq!(config.memory_max_chars_per_item, 180);
    assert_eq!(config.memory_embedding_model, None);
    assert_eq!(config.memory_embedding_api_base, None);
    assert_eq!(config.memory_embedding_api_key, None);
    assert!(config.response_cache_enabled);
    assert_eq!(config.response_cache_max_entries, 128);
    assert!(config.tool_result_cache_enabled);
    assert_eq!(config.tool_result_cache_max_entries, 256);
    assert_eq!(config.model_input_cost_per_million, None);
    assert_eq!(config.model_output_cost_per_million, None);
    assert_eq!(config.cost_budget_usd, None);
    assert_eq!(config.cost_alert_thresholds_percent, vec![80, 100]);
    assert_eq!(config.async_event_queue_capacity, 128);
    assert_eq!(config.async_event_handler_timeout_ms, Some(5_000));
    assert!(!config.async_event_block_on_full);
}

#[test]
fn unit_stream_retry_buffer_on_delta_suppresses_replayed_prefix() {
    let mut state = StreamingRetryBufferState::default();
    assert_eq!(
        stream_retry_buffer_on_delta(&mut state, "Hel"),
        Some("Hel".to_string())
    );

    state.reset_attempt();
    assert_eq!(stream_retry_buffer_on_delta(&mut state, "Hel"), None);
    assert_eq!(
        stream_retry_buffer_on_delta(&mut state, "lo"),
        Some("lo".to_string())
    );
}

#[tokio::test]
async fn regression_streaming_requests_bypass_response_cache() {
    let calls = Arc::new(AtomicUsize::new(0));
    let client = Arc::new(CountingStaticClient {
        calls: calls.clone(),
        response: ChatResponse {
            message: Message::assistant_text("streamed"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        },
    });
    let mut agent = Agent::new(client, AgentConfig::default());
    agent.append_message(Message::user("streaming cache bypass"));
    let baseline_messages = agent.messages.clone();
    let start_index = baseline_messages.len().saturating_sub(1);
    let sink = Arc::new(|_delta: String| {});

    let _ = agent
        .run_loop(start_index, Some(sink.clone()), false)
        .await
        .expect("first streamed run should succeed");
    agent.messages = baseline_messages;
    let _ = agent
        .run_loop(start_index, Some(sink), false)
        .await
        .expect("second streamed run should succeed");

    assert_eq!(calls.load(Ordering::Relaxed), 2);
}

#[tokio::test]
async fn integration_prompt_with_stream_emits_incremental_deltas() {
    let client = Arc::new(StreamingMockClient {
        response: ChatResponse {
            message: Message::assistant_text("Hello"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        },
        deltas: vec!["Hel".to_string(), "lo".to_string()],
    });

    let mut agent = Agent::new(client, AgentConfig::default());
    let streamed = Arc::new(Mutex::new(String::new()));
    let sink_streamed = streamed.clone();
    let sink = Arc::new(move |delta: String| {
        sink_streamed
            .lock()
            .expect("stream lock")
            .push_str(delta.as_str());
    });

    let new_messages = agent
        .prompt_with_stream("hello", Some(sink))
        .await
        .expect("prompt should succeed");

    assert_eq!(
        new_messages
            .last()
            .expect("assistant message")
            .text_content(),
        "Hello"
    );
    assert_eq!(streamed.lock().expect("stream lock").as_str(), "Hello");
}

#[tokio::test]
async fn functional_streaming_retry_replays_buffer_without_duplicate_output() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let client = Arc::new(RetryingStreamingClient {
        outcomes: AsyncMutex::new(VecDeque::from([
            RetryingStreamingOutcome {
                deltas: vec!["Hel".to_string()],
                response: Err(tau_ai::TauAiError::HttpStatus {
                    status: 503,
                    body: "transient".to_string(),
                }),
            },
            RetryingStreamingOutcome {
                deltas: vec!["Hello".to_string()],
                response: Ok(ChatResponse {
                    message: Message::assistant_text("Hello"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                }),
            },
        ])),
        attempts: attempts.clone(),
    });

    let mut agent = Agent::new(
        client,
        AgentConfig {
            request_max_retries: 1,
            request_retry_initial_backoff_ms: 1,
            request_retry_max_backoff_ms: 1,
            stream_retry_with_buffering: true,
            ..AgentConfig::default()
        },
    );
    let streamed = Arc::new(Mutex::new(String::new()));
    let streamed_sink = streamed.clone();
    let sink = Arc::new(move |delta: String| {
        streamed_sink
            .lock()
            .expect("stream lock")
            .push_str(delta.as_str());
    });

    let messages = agent
        .prompt_with_stream("hello", Some(sink))
        .await
        .expect("retrying stream should succeed");

    assert_eq!(messages.last().expect("assistant").text_content(), "Hello");
    assert_eq!(streamed.lock().expect("stream lock").as_str(), "Hello");
    assert_eq!(attempts.load(Ordering::Relaxed), 2);
}

#[tokio::test]
async fn integration_streaming_retry_with_buffering_continues_tool_turns() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let first_turn_retry = Message::assistant_blocks(vec![ContentBlock::ToolCall {
        id: "call_1".to_string(),
        name: "read".to_string(),
        arguments: serde_json::json!({ "path": "README.md" }),
    }]);
    let final_assistant = Message::assistant_text("done");
    let client = Arc::new(RetryingStreamingClient {
        outcomes: AsyncMutex::new(VecDeque::from([
            RetryingStreamingOutcome {
                deltas: vec!["To".to_string()],
                response: Err(tau_ai::TauAiError::HttpStatus {
                    status: 503,
                    body: "temporary".to_string(),
                }),
            },
            RetryingStreamingOutcome {
                deltas: vec!["Tool ".to_string()],
                response: Ok(ChatResponse {
                    message: first_turn_retry,
                    finish_reason: Some("tool_calls".to_string()),
                    usage: ChatUsage::default(),
                }),
            },
            RetryingStreamingOutcome {
                deltas: vec!["done".to_string()],
                response: Ok(ChatResponse {
                    message: final_assistant,
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                }),
            },
        ])),
        attempts: attempts.clone(),
    });

    let mut agent = Agent::new(
        client,
        AgentConfig {
            request_max_retries: 1,
            request_retry_initial_backoff_ms: 1,
            request_retry_max_backoff_ms: 1,
            stream_retry_with_buffering: true,
            ..AgentConfig::default()
        },
    );
    agent.register_tool(ReadTool);

    let streamed = Arc::new(Mutex::new(String::new()));
    let streamed_sink = streamed.clone();
    let sink = Arc::new(move |delta: String| {
        streamed_sink
            .lock()
            .expect("stream lock")
            .push_str(delta.as_str());
    });

    let messages = agent
        .prompt_with_stream("read file", Some(sink))
        .await
        .expect("streaming retry with tools should succeed");

    assert_eq!(messages.last().expect("assistant").text_content(), "done");
    assert!(
        messages
            .iter()
            .any(|message| message.role == MessageRole::Tool),
        "tool turn should still execute after a retried streaming failure"
    );
    assert_eq!(streamed.lock().expect("stream lock").as_str(), "Tool done");
    assert_eq!(attempts.load(Ordering::Relaxed), 3);
}

#[tokio::test]
async fn regression_streaming_retry_disabled_fails_without_retrying_stream() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let client = Arc::new(RetryingStreamingClient {
        outcomes: AsyncMutex::new(VecDeque::from([
            RetryingStreamingOutcome {
                deltas: vec!["Hel".to_string()],
                response: Err(tau_ai::TauAiError::HttpStatus {
                    status: 503,
                    body: "temporary".to_string(),
                }),
            },
            RetryingStreamingOutcome {
                deltas: vec!["Hello".to_string()],
                response: Ok(ChatResponse {
                    message: Message::assistant_text("Hello"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                }),
            },
        ])),
        attempts: attempts.clone(),
    });

    let mut agent = Agent::new(
        client,
        AgentConfig {
            request_max_retries: 1,
            request_retry_initial_backoff_ms: 1,
            request_retry_max_backoff_ms: 1,
            stream_retry_with_buffering: false,
            ..AgentConfig::default()
        },
    );
    let streamed = Arc::new(Mutex::new(String::new()));
    let streamed_sink = streamed.clone();
    let sink = Arc::new(move |delta: String| {
        streamed_sink
            .lock()
            .expect("stream lock")
            .push_str(delta.as_str());
    });

    let error = agent
        .prompt_with_stream("hello", Some(sink))
        .await
        .expect_err("disabled buffering should not retry streaming errors");
    assert!(matches!(
        error,
        AgentError::Ai(tau_ai::TauAiError::HttpStatus { status: 503, .. })
    ));
    assert_eq!(streamed.lock().expect("stream lock").as_str(), "Hel");
    assert_eq!(attempts.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn functional_request_timeout_fails_closed_for_slow_provider() {
    let mut agent = Agent::new(
        Arc::new(DelayedEchoClient { delay_ms: 80 }),
        AgentConfig {
            request_max_retries: 0,
            request_timeout_ms: Some(10),
            ..AgentConfig::default()
        },
    );

    let error = agent
        .prompt("timeout please")
        .await
        .expect_err("slow provider should time out");
    match error {
        AgentError::RequestTimeout {
            timeout_ms: 10,
            attempt: 1,
        } => {}
        other => panic!("expected request timeout on first attempt, got {other:?}"),
    }
}

#[tokio::test]
async fn functional_token_budget_exceeded_fails_before_request_dispatch() {
    let client = Arc::new(CapturingMockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: Message::assistant_text("should-not-run"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }])),
        requests: AsyncMutex::new(Vec::new()),
    });

    let mut agent = Agent::new(
        client.clone(),
        AgentConfig {
            system_prompt: String::new(),
            max_estimated_input_tokens: Some(1),
            ..AgentConfig::default()
        },
    );
    let error = agent
        .prompt("this prompt should exceed budget")
        .await
        .expect_err("token budget should fail closed");
    assert!(matches!(error, AgentError::TokenBudgetExceeded { .. }));
    assert!(
        client.requests.lock().await.is_empty(),
        "request should not be dispatched when budget check fails"
    );
}

#[tokio::test]
async fn integration_total_token_budget_enforces_max_tokens_headroom() {
    let client = Arc::new(CapturingMockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: Message::assistant_text("should-not-run"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }])),
        requests: AsyncMutex::new(Vec::new()),
    });

    let mut agent = Agent::new(
        client.clone(),
        AgentConfig {
            system_prompt: String::new(),
            max_tokens: Some(64),
            max_estimated_input_tokens: Some(10_000),
            max_estimated_total_tokens: Some(30),
            ..AgentConfig::default()
        },
    );
    let error = agent
        .prompt("small prompt")
        .await
        .expect_err("max_tokens should count against total budget");
    match error {
        AgentError::TokenBudgetExceeded {
            max_total_tokens: 30,
            ..
        } => {}
        other => panic!("expected total token budget failure, got {other:?}"),
    }
    assert!(
        client.requests.lock().await.is_empty(),
        "request should not be dispatched when total budget is exceeded"
    );
}

#[test]
fn unit_estimate_usage_cost_usd_applies_input_and_output_rates() {
    let usage = ChatUsage {
        input_tokens: 2_000,
        output_tokens: 500,
        total_tokens: 2_500,
    };
    let cost = estimate_usage_cost_usd(&usage, Some(1.5), Some(6.0));
    let expected = (2_000.0 * 1.5 + 500.0 * 6.0) / 1_000_000.0;
    assert!((cost - expected).abs() < 1e-12);
}

#[test]
fn unit_normalize_cost_alert_thresholds_filters_invalid_and_deduplicates() {
    assert_eq!(
        normalize_cost_alert_thresholds(&[0, 80, 80, 120, 100]),
        vec![80, 100]
    );
    assert_eq!(normalize_cost_alert_thresholds(&[]), vec![100]);
}

#[tokio::test]
async fn functional_prompt_emits_cost_update_event_when_model_pricing_present() {
    let client = Arc::new(MockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: Message::assistant_text("done"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage {
                input_tokens: 200,
                output_tokens: 100,
                total_tokens: 300,
            },
        }])),
    });
    let mut agent = Agent::new(
        client,
        AgentConfig {
            model_input_cost_per_million: Some(2.0),
            model_output_cost_per_million: Some(4.0),
            ..AgentConfig::default()
        },
    );
    let observed = Arc::new(Mutex::new(Vec::<(usize, f64, f64, Option<f64>)>::new()));
    let observed_clone = observed.clone();
    agent.subscribe(move |event| {
        if let AgentEvent::CostUpdated {
            turn,
            turn_cost_usd,
            cumulative_cost_usd,
            budget_usd,
        } = event
        {
            observed_clone.lock().expect("events lock").push((
                *turn,
                *turn_cost_usd,
                *cumulative_cost_usd,
                *budget_usd,
            ));
        }
    });

    let _ = agent
        .prompt("price this run")
        .await
        .expect("prompt should succeed");

    let snapshot = agent.cost_snapshot();
    let expected = (200.0 * 2.0 + 100.0 * 4.0) / 1_000_000.0;
    assert_eq!(snapshot.input_tokens, 200);
    assert_eq!(snapshot.output_tokens, 100);
    assert_eq!(snapshot.total_tokens, 300);
    assert!((snapshot.estimated_cost_usd - expected).abs() < 1e-12);
    assert_eq!(snapshot.budget_usd, None);
    assert_eq!(snapshot.budget_utilization, None);

    let events = observed.lock().expect("events lock").clone();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, 1);
    assert!((events[0].1 - expected).abs() < 1e-12);
    assert!((events[0].2 - expected).abs() < 1e-12);
    assert_eq!(events[0].3, None);
}

#[tokio::test]
async fn integration_budget_alerts_emit_once_per_threshold_across_multiple_prompts() {
    let client = Arc::new(MockClient {
        responses: AsyncMutex::new(VecDeque::from([
            ChatResponse {
                message: Message::assistant_text("first"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage {
                    input_tokens: 80_000,
                    output_tokens: 0,
                    total_tokens: 80_000,
                },
            },
            ChatResponse {
                message: Message::assistant_text("second"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage {
                    input_tokens: 40_000,
                    output_tokens: 0,
                    total_tokens: 40_000,
                },
            },
            ChatResponse {
                message: Message::assistant_text("third"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage {
                    input_tokens: 40_000,
                    output_tokens: 0,
                    total_tokens: 40_000,
                },
            },
        ])),
    });
    let mut agent = Agent::new(
        client,
        AgentConfig {
            model_input_cost_per_million: Some(10.0),
            model_output_cost_per_million: Some(0.0),
            cost_budget_usd: Some(1.5),
            cost_alert_thresholds_percent: vec![50, 80, 100],
            ..AgentConfig::default()
        },
    );
    let thresholds = Arc::new(Mutex::new(Vec::<u8>::new()));
    let thresholds_clone = thresholds.clone();
    agent.subscribe(move |event| {
        if let AgentEvent::CostBudgetAlert {
            threshold_percent, ..
        } = event
        {
            thresholds_clone
                .lock()
                .expect("threshold lock")
                .push(*threshold_percent);
        }
    });

    let _ = agent.prompt("step 1").await.expect("first prompt");
    let _ = agent.prompt("step 2").await.expect("second prompt");
    let _ = agent.prompt("step 3").await.expect("third prompt");

    let snapshot = agent.cost_snapshot();
    assert!((snapshot.estimated_cost_usd - 1.6).abs() < 1e-9);
    assert_eq!(snapshot.budget_usd, Some(1.5));
    let utilization = snapshot.budget_utilization.expect("utilization");
    assert!(utilization > 1.0);

    assert_eq!(
        thresholds.lock().expect("threshold lock").as_slice(),
        &[50, 80, 100]
    );
}

#[tokio::test]
async fn regression_cost_budget_alert_threshold_normalization_avoids_duplicates() {
    let client = Arc::new(MockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: Message::assistant_text("done"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage {
                input_tokens: 150_000,
                output_tokens: 0,
                total_tokens: 150_000,
            },
        }])),
    });
    let mut agent = Agent::new(
        client,
        AgentConfig {
            model_input_cost_per_million: Some(10.0),
            cost_budget_usd: Some(1.0),
            cost_alert_thresholds_percent: vec![0, 80, 80, 120, 100],
            ..AgentConfig::default()
        },
    );
    let thresholds = Arc::new(Mutex::new(Vec::<u8>::new()));
    let thresholds_clone = thresholds.clone();
    agent.subscribe(move |event| {
        if let AgentEvent::CostBudgetAlert {
            threshold_percent, ..
        } = event
        {
            thresholds_clone
                .lock()
                .expect("threshold lock")
                .push(*threshold_percent);
        }
    });

    let _ = agent
        .prompt("single run")
        .await
        .expect("prompt should succeed");
    assert_eq!(
        thresholds.lock().expect("threshold lock").as_slice(),
        &[80, 100]
    );
}

#[test]
fn unit_build_structured_output_retry_prompt_includes_error_and_schema() {
    let schema = serde_json::json!({
        "type": "object",
        "required": ["mode"]
    });
    let prompt = build_structured_output_retry_prompt(&schema, "did not contain parseable JSON");
    assert!(prompt.contains("did not contain parseable JSON"));
    assert!(prompt.contains("\"required\":[\"mode\"]"));
    assert!(prompt.contains("reply with only valid JSON"));
}

#[tokio::test]
async fn regression_prompt_json_retry_exhaustion_fails_closed() {
    let client = Arc::new(CapturingMockClient {
        responses: AsyncMutex::new(VecDeque::from([
            ChatResponse {
                message: Message::assistant_text("still-not-json"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: Message::assistant_text("again-not-json"),
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
    let schema = serde_json::json!({ "type": "object" });

    let error = agent
        .prompt_json("return object", &schema)
        .await
        .expect_err("non-json output must fail after retries are exhausted");
    assert!(matches!(error, AgentError::StructuredOutput(_)));
    assert!(error.to_string().contains("did not contain parseable JSON"));

    let requests = client.requests.lock().await;
    assert_eq!(requests.len(), 2, "expected one retry attempt");
}

#[tokio::test]
async fn regression_retry_transient_request_failures_and_recover_response() {
    let client = Arc::new(RetryThenSuccessClient {
        remaining_failures: AsyncMutex::new(1),
        attempts: AsyncMutex::new(0),
        response: ChatResponse {
            message: Message::assistant_text("recovered"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        },
    });
    let mut agent = Agent::new(
        client.clone(),
        AgentConfig {
            request_max_retries: 2,
            request_retry_initial_backoff_ms: 1,
            request_retry_max_backoff_ms: 2,
            ..AgentConfig::default()
        },
    );

    let messages = agent
        .prompt("retry please")
        .await
        .expect("prompt should recover");
    assert_eq!(
        messages.last().expect("assistant response").text_content(),
        "recovered"
    );
    assert_eq!(*client.attempts.lock().await, 2);
}

#[tokio::test]
async fn regression_request_timeout_retries_and_recovers_when_next_attempt_is_fast() {
    let client = Arc::new(TimeoutThenSuccessClient {
        delays_ms: AsyncMutex::new(VecDeque::from([40, 0])),
        attempts: AsyncMutex::new(0),
        response: ChatResponse {
            message: Message::assistant_text("timeout-recovered"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        },
    });
    let mut agent = Agent::new(
        client.clone(),
        AgentConfig {
            request_max_retries: 1,
            request_retry_initial_backoff_ms: 1,
            request_retry_max_backoff_ms: 1,
            request_timeout_ms: Some(10),
            ..AgentConfig::default()
        },
    );

    let messages = agent
        .prompt("recover after timeout")
        .await
        .expect("second attempt should succeed");
    assert_eq!(
        messages.last().expect("assistant response").text_content(),
        "timeout-recovered"
    );
    assert_eq!(*client.attempts.lock().await, 2);
}

#[tokio::test]
async fn regression_token_budget_none_disables_estimation_gate() {
    let client = Arc::new(MockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: Message::assistant_text("ok"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }])),
    });
    let mut agent = Agent::new(
        client,
        AgentConfig {
            system_prompt: String::new(),
            max_estimated_input_tokens: None,
            max_estimated_total_tokens: None,
            ..AgentConfig::default()
        },
    );
    let oversized_prompt = "x".repeat(250_000);
    let messages = agent
        .prompt(oversized_prompt)
        .await
        .expect("token gate disabled should allow prompt");
    assert_eq!(
        messages.last().expect("assistant response").text_content(),
        "ok"
    );
}

#[tokio::test]
async fn integration_tool_timeout_returns_error_tool_message_and_continues_turn() {
    let first_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
        id: "call_1".to_string(),
        name: "slow_read".to_string(),
        arguments: serde_json::json!({ "path": "README.md" }),
    }]);
    let second_assistant = Message::assistant_text("continued after timeout");
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
            tool_timeout_ms: Some(10),
            ..AgentConfig::default()
        },
    );
    agent.register_tool(SlowReadTool { delay_ms: 75 });

    let messages = agent
        .prompt("slow read")
        .await
        .expect("prompt should continue after tool timeout");
    let tool_message = messages
        .iter()
        .find(|message| message.role == MessageRole::Tool)
        .expect("tool result should be present");
    assert!(tool_message.is_error);
    assert!(tool_message.text_content().contains("timed out after 10ms"));
    assert_eq!(
        messages.last().expect("assistant response").text_content(),
        "continued after timeout"
    );
}
