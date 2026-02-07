use super::*;

type FallbackEventSink = Arc<dyn Fn(serde_json::Value) + Send + Sync>;

#[derive(Clone)]
pub(crate) struct ClientRoute {
    pub(crate) provider: Provider,
    pub(crate) model: String,
    pub(crate) client: Arc<dyn LlmClient>,
}

impl ClientRoute {
    fn model_ref(&self) -> String {
        format!("{}/{}", self.provider, self.model)
    }
}

pub(crate) struct FallbackRoutingClient {
    routes: Vec<ClientRoute>,
    event_sink: Option<FallbackEventSink>,
}

impl FallbackRoutingClient {
    pub(crate) fn new(routes: Vec<ClientRoute>, event_sink: Option<FallbackEventSink>) -> Self {
        Self { routes, event_sink }
    }

    fn emit_fallback_event(
        &self,
        from: &ClientRoute,
        to: &ClientRoute,
        error: &PiAiError,
        fallback_index: usize,
    ) {
        let Some(sink) = &self.event_sink else {
            return;
        };
        let (error_kind, status) = fallback_error_metadata(error);
        sink(serde_json::json!({
            "type": "provider_fallback",
            "from_model": from.model_ref(),
            "to_model": to.model_ref(),
            "error_kind": error_kind,
            "status": status,
            "fallback_index": fallback_index,
        }));
    }

    async fn complete_inner(
        &self,
        request: ChatRequest,
        on_delta: Option<StreamDeltaHandler>,
    ) -> Result<ChatResponse, PiAiError> {
        if self.routes.is_empty() {
            return Err(PiAiError::InvalidResponse(
                "no provider routes configured".to_string(),
            ));
        }

        for (index, route) in self.routes.iter().enumerate() {
            let mut routed_request = request.clone();
            routed_request.model = route.model.clone();

            let response = if let Some(stream_handler) = on_delta.clone() {
                route
                    .client
                    .complete_with_stream(routed_request, Some(stream_handler))
                    .await
            } else {
                route.client.complete(routed_request).await
            };

            match response {
                Ok(response) => return Ok(response),
                Err(error) => {
                    let Some(next_route) = self.routes.get(index + 1) else {
                        return Err(error);
                    };
                    if is_retryable_provider_error(&error) {
                        self.emit_fallback_event(route, next_route, &error, index + 1);
                        continue;
                    }
                    return Err(error);
                }
            }
        }

        Err(PiAiError::InvalidResponse(
            "provider fallback chain exhausted unexpectedly".to_string(),
        ))
    }
}

#[async_trait]
impl LlmClient for FallbackRoutingClient {
    async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, PiAiError> {
        self.complete_inner(request, None).await
    }

    async fn complete_with_stream(
        &self,
        request: ChatRequest,
        on_delta: Option<StreamDeltaHandler>,
    ) -> Result<ChatResponse, PiAiError> {
        self.complete_inner(request, on_delta).await
    }
}

fn is_retryable_status(status: u16) -> bool {
    status == 408 || status == 409 || status == 425 || status == 429 || status >= 500
}

pub(crate) fn is_retryable_provider_error(error: &PiAiError) -> bool {
    match error {
        PiAiError::HttpStatus { status, .. } => is_retryable_status(*status),
        PiAiError::Http(inner) => {
            inner.is_timeout() || inner.is_connect() || inner.is_request() || inner.is_body()
        }
        _ => false,
    }
}

fn fallback_error_metadata(error: &PiAiError) -> (&'static str, Option<u16>) {
    match error {
        PiAiError::HttpStatus { status, .. } => ("http_status", Some(*status)),
        PiAiError::Http(inner) if inner.is_timeout() => ("http_timeout", None),
        PiAiError::Http(inner) if inner.is_connect() => ("http_connect", None),
        PiAiError::Http(inner) if inner.is_request() => ("http_request", None),
        PiAiError::Http(inner) if inner.is_body() => ("http_body", None),
        PiAiError::Http(_) => ("http_other", None),
        PiAiError::MissingApiKey => ("missing_api_key", None),
        PiAiError::Serde(_) => ("serde", None),
        PiAiError::InvalidResponse(_) => ("invalid_response", None),
    }
}

pub(crate) fn resolve_fallback_models(cli: &Cli, primary: &ModelRef) -> Result<Vec<ModelRef>> {
    let mut resolved = Vec::new();
    for raw in &cli.fallback_model {
        let parsed = ModelRef::parse(raw)
            .map_err(|error| anyhow!("failed to parse --fallback-model '{}': {error}", raw))?;

        if parsed.provider == primary.provider && parsed.model == primary.model {
            continue;
        }

        if resolved.iter().any(|existing: &ModelRef| {
            existing.provider == parsed.provider && existing.model == parsed.model
        }) {
            continue;
        }

        resolved.push(parsed);
    }
    Ok(resolved)
}

pub(crate) fn build_client_with_fallbacks(
    cli: &Cli,
    primary: &ModelRef,
    fallback_models: &[ModelRef],
) -> Result<Arc<dyn LlmClient>> {
    let primary_client = build_provider_client(cli, primary.provider)
        .with_context(|| format!("failed to create {} client", primary.provider))?;
    if fallback_models.is_empty() {
        return Ok(primary_client);
    }

    let mut provider_clients: Vec<(Provider, Arc<dyn LlmClient>)> =
        vec![(primary.provider, primary_client.clone())];
    let mut routes = vec![ClientRoute {
        provider: primary.provider,
        model: primary.model.clone(),
        client: primary_client,
    }];

    for model_ref in fallback_models {
        let client = if let Some((_, existing)) = provider_clients
            .iter()
            .find(|(provider, _)| *provider == model_ref.provider)
        {
            existing.clone()
        } else {
            let created = build_provider_client(cli, model_ref.provider).with_context(|| {
                format!(
                    "failed to create {} client for fallback model '{}'",
                    model_ref.provider, model_ref.model
                )
            })?;
            provider_clients.push((model_ref.provider, created.clone()));
            created
        };

        routes.push(ClientRoute {
            provider: model_ref.provider,
            model: model_ref.model.clone(),
            client,
        });
    }

    let event_sink = if cli.json_events {
        Some(Arc::new(|event| println!("{event}")) as FallbackEventSink)
    } else {
        None
    };
    Ok(Arc::new(FallbackRoutingClient::new(routes, event_sink)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    #[derive(Clone)]
    struct MockLlmClient {
        complete_responses: Arc<Mutex<VecDeque<Result<ChatResponse, PiAiError>>>>,
        stream_responses: Arc<Mutex<VecDeque<MockStreamResponse>>>,
        observed_models: Arc<Mutex<Vec<String>>>,
    }

    struct MockStreamResponse {
        deltas: Vec<String>,
        result: Result<ChatResponse, PiAiError>,
    }

    impl MockLlmClient {
        fn new(
            complete_responses: Vec<Result<ChatResponse, PiAiError>>,
            stream_responses: Vec<MockStreamResponse>,
        ) -> Self {
            Self {
                complete_responses: Arc::new(Mutex::new(complete_responses.into())),
                stream_responses: Arc::new(Mutex::new(stream_responses.into())),
                observed_models: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn observed_models(&self) -> Vec<String> {
            self.observed_models
                .lock()
                .expect("observed models lock")
                .clone()
        }
    }

    #[async_trait]
    impl LlmClient for MockLlmClient {
        async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, PiAiError> {
            self.observed_models
                .lock()
                .expect("observed models lock")
                .push(request.model);

            self.complete_responses
                .lock()
                .expect("complete responses lock")
                .pop_front()
                .unwrap_or_else(|| {
                    Err(PiAiError::InvalidResponse(
                        "no mock complete response configured".to_string(),
                    ))
                })
        }

        async fn complete_with_stream(
            &self,
            request: ChatRequest,
            on_delta: Option<StreamDeltaHandler>,
        ) -> Result<ChatResponse, PiAiError> {
            self.observed_models
                .lock()
                .expect("observed models lock")
                .push(request.model);

            let next = self
                .stream_responses
                .lock()
                .expect("stream responses lock")
                .pop_front()
                .unwrap_or(MockStreamResponse {
                    deltas: Vec::new(),
                    result: Err(PiAiError::InvalidResponse(
                        "no mock stream response configured".to_string(),
                    )),
                });

            if let Some(handler) = on_delta {
                for delta in next.deltas {
                    handler(delta);
                }
            }

            next.result
        }
    }

    fn assistant_text_response(text: &str) -> ChatResponse {
        ChatResponse {
            message: Message::assistant_text(text),
            finish_reason: Some("stop".to_string()),
            usage: Default::default(),
        }
    }

    fn test_request() -> ChatRequest {
        ChatRequest {
            model: "placeholder-model".to_string(),
            messages: vec![Message::user("hello")],
            tools: Vec::new(),
            max_tokens: None,
            temperature: None,
        }
    }

    #[test]
    fn unit_retryable_provider_error_classifies_expected_statuses() {
        assert!(is_retryable_provider_error(&PiAiError::HttpStatus {
            status: 408,
            body: "timeout".to_string(),
        }));
        assert!(is_retryable_provider_error(&PiAiError::HttpStatus {
            status: 409,
            body: "conflict".to_string(),
        }));
        assert!(is_retryable_provider_error(&PiAiError::HttpStatus {
            status: 425,
            body: "too early".to_string(),
        }));
        assert!(is_retryable_provider_error(&PiAiError::HttpStatus {
            status: 429,
            body: "rate limit".to_string(),
        }));
        assert!(is_retryable_provider_error(&PiAiError::HttpStatus {
            status: 500,
            body: "server error".to_string(),
        }));
        assert!(!is_retryable_provider_error(&PiAiError::HttpStatus {
            status: 401,
            body: "unauthorized".to_string(),
        }));
        assert!(!is_retryable_provider_error(&PiAiError::InvalidResponse(
            "bad payload".to_string(),
        )));
    }

    #[tokio::test]
    async fn functional_fallback_client_handoffs_on_retryable_error_and_emits_event() {
        let primary = MockLlmClient::new(
            vec![Err(PiAiError::HttpStatus {
                status: 429,
                body: "rate limited".to_string(),
            })],
            Vec::new(),
        );
        let secondary =
            MockLlmClient::new(vec![Ok(assistant_text_response("fallback ok"))], Vec::new());

        let events = Arc::new(Mutex::new(Vec::<Value>::new()));
        let events_sink = events.clone();
        let event_sink: FallbackEventSink =
            Arc::new(move |event| events_sink.lock().expect("events lock").push(event));

        let router = FallbackRoutingClient::new(
            vec![
                ClientRoute {
                    provider: Provider::OpenAi,
                    model: "gpt-4o-mini".to_string(),
                    client: Arc::new(primary.clone()),
                },
                ClientRoute {
                    provider: Provider::Anthropic,
                    model: "claude-sonnet-4-20250514".to_string(),
                    client: Arc::new(secondary.clone()),
                },
            ],
            Some(event_sink),
        );

        let response = router
            .complete(test_request())
            .await
            .expect("fallback route should succeed");

        assert_eq!(response.message.text_content(), "fallback ok");
        assert_eq!(primary.observed_models(), vec!["gpt-4o-mini".to_string()]);
        assert_eq!(
            secondary.observed_models(),
            vec!["claude-sonnet-4-20250514".to_string()]
        );

        let events = events.lock().expect("events lock");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["type"], "provider_fallback");
        assert_eq!(events[0]["from_model"], "openai/gpt-4o-mini");
        assert_eq!(events[0]["to_model"], "anthropic/claude-sonnet-4-20250514");
        assert_eq!(events[0]["error_kind"], "http_status");
        assert_eq!(events[0]["status"], 429);
        assert_eq!(events[0]["fallback_index"], 1);
    }

    #[tokio::test]
    async fn integration_streaming_fallback_preserves_deltas_and_returns_response() {
        let primary = MockLlmClient::new(
            Vec::new(),
            vec![MockStreamResponse {
                deltas: Vec::new(),
                result: Err(PiAiError::HttpStatus {
                    status: 503,
                    body: "service unavailable".to_string(),
                }),
            }],
        );
        let secondary = MockLlmClient::new(
            Vec::new(),
            vec![MockStreamResponse {
                deltas: vec!["Hel".to_string(), "lo".to_string()],
                result: Ok(assistant_text_response("Hello")),
            }],
        );

        let router = FallbackRoutingClient::new(
            vec![
                ClientRoute {
                    provider: Provider::OpenAi,
                    model: "gpt-4o-mini".to_string(),
                    client: Arc::new(primary.clone()),
                },
                ClientRoute {
                    provider: Provider::Google,
                    model: "gemini-2.5-pro".to_string(),
                    client: Arc::new(secondary.clone()),
                },
            ],
            None,
        );

        let deltas = Arc::new(Mutex::new(String::new()));
        let delta_sink = deltas.clone();
        let sink: StreamDeltaHandler = Arc::new(move |delta| {
            delta_sink.lock().expect("delta lock").push_str(&delta);
        });

        let response = router
            .complete_with_stream(test_request(), Some(sink))
            .await
            .expect("streaming fallback should succeed");

        assert_eq!(deltas.lock().expect("delta lock").as_str(), "Hello");
        assert_eq!(response.message.text_content(), "Hello");
        assert_eq!(primary.observed_models(), vec!["gpt-4o-mini".to_string()]);
        assert_eq!(
            secondary.observed_models(),
            vec!["gemini-2.5-pro".to_string()]
        );
    }

    #[tokio::test]
    async fn regression_non_retryable_error_does_not_fallback_to_next_route() {
        let primary = MockLlmClient::new(
            vec![Err(PiAiError::HttpStatus {
                status: 401,
                body: "unauthorized".to_string(),
            })],
            Vec::new(),
        );
        let secondary =
            MockLlmClient::new(vec![Ok(assistant_text_response("unexpected"))], Vec::new());

        let router = FallbackRoutingClient::new(
            vec![
                ClientRoute {
                    provider: Provider::OpenAi,
                    model: "gpt-4o-mini".to_string(),
                    client: Arc::new(primary.clone()),
                },
                ClientRoute {
                    provider: Provider::Anthropic,
                    model: "claude-sonnet-4-20250514".to_string(),
                    client: Arc::new(secondary.clone()),
                },
            ],
            None,
        );

        let error = router
            .complete(test_request())
            .await
            .expect_err("non-retryable failure should be returned directly");

        match error {
            PiAiError::HttpStatus { status, body } => {
                assert_eq!(status, 401);
                assert!(body.contains("unauthorized"));
            }
            other => panic!("expected HttpStatus error, got {other:?}"),
        }

        assert_eq!(primary.observed_models(), vec!["gpt-4o-mini".to_string()]);
        assert!(secondary.observed_models().is_empty());
    }
}
