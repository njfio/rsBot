use super::*;

#[test]
fn unit_emit_isolates_panicking_handler_and_invokes_remaining_subscribers() {
    let mut agent = Agent::new(Arc::new(EchoClient), AgentConfig::default());
    let observed = Arc::new(AtomicUsize::new(0));

    agent.subscribe(|event| {
        if matches!(event, AgentEvent::AgentStart) {
            panic!("forced event handler panic");
        }
    });
    let observed_clone = observed.clone();
    agent.subscribe(move |_event| {
        observed_clone.fetch_add(1, Ordering::Relaxed);
    });

    agent.emit(AgentEvent::AgentStart);
    assert_eq!(observed.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn functional_async_subscriber_receives_events_and_records_metrics() {
    let client = Arc::new(MockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: Message::assistant_text("ok"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }])),
    });
    let mut agent = Agent::new(client, AgentConfig::default());
    let observed = Arc::new(AtomicUsize::new(0));
    let observed_clone = observed.clone();
    agent.subscribe_async(move |_event| {
        let observed_clone = observed_clone.clone();
        async move {
            observed_clone.fetch_add(1, Ordering::Relaxed);
        }
    });

    let _ = agent.prompt("hello").await.expect("prompt should succeed");
    let deadline = tokio::time::Instant::now() + Duration::from_millis(250);
    while tokio::time::Instant::now() < deadline {
        let metrics = agent.async_event_metrics();
        if observed.load(Ordering::Relaxed) > 0 && metrics.completed > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(
        observed.load(Ordering::Relaxed) > 0,
        "async handler should observe at least one event"
    );
    let metrics = agent.async_event_metrics();
    assert!(metrics.enqueued > 0);
    assert!(metrics.completed > 0);
    assert_eq!(metrics.dropped_full, 0);
}

#[tokio::test]
async fn integration_async_subscriber_backpressure_drops_when_queue_is_full() {
    let mut agent = Agent::new(
        Arc::new(EchoClient),
        AgentConfig {
            async_event_queue_capacity: 1,
            async_event_block_on_full: false,
            async_event_handler_timeout_ms: None,
            ..AgentConfig::default()
        },
    );
    agent.subscribe_async(|_event| async move {
        tokio::time::sleep(Duration::from_millis(80)).await;
    });

    for _ in 0..20 {
        agent.emit(AgentEvent::AgentStart);
    }
    tokio::time::sleep(Duration::from_millis(250)).await;
    let metrics = agent.async_event_metrics();
    assert!(metrics.enqueued >= 1);
    assert!(metrics.dropped_full > 0);
}

#[tokio::test]
async fn regression_async_subscriber_timeout_and_panic_are_isolated() {
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
            async_event_queue_capacity: 16,
            async_event_handler_timeout_ms: Some(20),
            ..AgentConfig::default()
        },
    );
    agent.subscribe_async(|_event| async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
    });
    agent.subscribe_async(|_event| async move {
        panic!("forced async handler panic");
    });

    let _ = agent
        .prompt("trigger async handlers")
        .await
        .expect("prompt should remain healthy");
    tokio::time::sleep(Duration::from_millis(250)).await;
    let metrics = agent.async_event_metrics();
    assert!(metrics.timed_out > 0);
    assert!(metrics.panicked > 0);
}

#[tokio::test]
async fn functional_prompt_completes_when_event_handler_panics() {
    let client = Arc::new(MockClient {
        responses: AsyncMutex::new(VecDeque::from([ChatResponse {
            message: Message::assistant_text("ok"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }])),
    });
    let mut agent = Agent::new(client, AgentConfig::default());
    agent.subscribe(|event| {
        if matches!(event, AgentEvent::AgentStart) {
            panic!("panic in functional handler");
        }
    });

    let messages = agent
        .prompt("hello")
        .await
        .expect("panic in event handler should not abort prompt");
    assert_eq!(messages.last().expect("assistant").text_content(), "ok");
}

#[tokio::test]
async fn integration_tool_turn_completes_when_handler_panics_on_tool_events() {
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
    agent.subscribe(|event| {
        if matches!(event, AgentEvent::ToolExecutionStart { .. }) {
            panic!("panic on tool start");
        }
    });

    let messages = agent
        .prompt("read")
        .await
        .expect("tool turn should survive panicking handler");
    assert!(
        messages
            .iter()
            .any(|message| message.role == MessageRole::Tool),
        "tool result should still be recorded"
    );
    assert_eq!(messages.last().expect("assistant").text_content(), "done");
}

#[tokio::test]
async fn regression_panicking_handler_does_not_break_subsequent_prompts() {
    let client = Arc::new(MockClient {
        responses: AsyncMutex::new(VecDeque::from([
            ChatResponse {
                message: Message::assistant_text("first"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: Message::assistant_text("second"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
        ])),
    });
    let mut agent = Agent::new(client, AgentConfig::default());
    let event_count = Arc::new(AtomicUsize::new(0));
    let event_count_clone = event_count.clone();
    agent.subscribe(move |_event| {
        event_count_clone.fetch_add(1, Ordering::Relaxed);
    });
    agent.subscribe(|event| {
        if matches!(event, AgentEvent::AgentStart) {
            panic!("panic every run");
        }
    });

    let first = agent
        .prompt("one")
        .await
        .expect("first prompt should succeed");
    let second = agent
        .prompt("two")
        .await
        .expect("second prompt should succeed");
    assert_eq!(first.last().expect("assistant").text_content(), "first");
    assert_eq!(second.last().expect("assistant").text_content(), "second");
    assert!(
        event_count.load(Ordering::Relaxed) > 0,
        "non-panicking handler should keep receiving events across runs"
    );
}
