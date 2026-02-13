use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use tau_ai::{
    ChatRequest, ChatResponse, LlmClient, ModelRef, Provider, StreamDeltaHandler, TauAiError,
};
use tau_cli::Cli;
use tau_core::current_unix_timestamp_ms;

use crate::client::build_provider_client;

type FallbackEventSink = Arc<dyn Fn(serde_json::Value) + Send + Sync>;
type ClockFn = Arc<dyn Fn() -> u64 + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Public struct `CircuitBreakerConfig` used across Tau components.
pub struct CircuitBreakerConfig {
    pub enabled: bool,
    pub failure_threshold: usize,
    pub cooldown_ms: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            failure_threshold: 3,
            cooldown_ms: 30_000,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct RouteCircuitState {
    consecutive_failures: usize,
    open_until_unix_ms: Option<u64>,
}

#[derive(Clone)]
/// Public struct `ClientRoute` used across Tau components.
pub struct ClientRoute {
    pub provider: Provider,
    pub model: String,
    pub client: Arc<dyn LlmClient>,
}

impl ClientRoute {
    fn model_ref(&self) -> String {
        format!("{}/{}", self.provider, self.model)
    }
}

/// Public struct `FallbackRoutingClient` used across Tau components.
pub struct FallbackRoutingClient {
    routes: Vec<ClientRoute>,
    event_sink: Option<FallbackEventSink>,
    circuit_breaker: CircuitBreakerConfig,
    route_circuit_state: Mutex<Vec<RouteCircuitState>>,
    clock: ClockFn,
}

impl FallbackRoutingClient {
    pub fn new(routes: Vec<ClientRoute>, event_sink: Option<FallbackEventSink>) -> Self {
        Self::with_circuit_breaker(routes, event_sink, CircuitBreakerConfig::default())
    }

    pub fn with_circuit_breaker(
        routes: Vec<ClientRoute>,
        event_sink: Option<FallbackEventSink>,
        circuit_breaker: CircuitBreakerConfig,
    ) -> Self {
        Self::new_with_clock(
            routes,
            event_sink,
            circuit_breaker,
            Arc::new(current_unix_timestamp_ms),
        )
    }

    fn new_with_clock(
        routes: Vec<ClientRoute>,
        event_sink: Option<FallbackEventSink>,
        circuit_breaker: CircuitBreakerConfig,
        clock: ClockFn,
    ) -> Self {
        Self {
            route_circuit_state: Mutex::new(vec![RouteCircuitState::default(); routes.len()]),
            routes,
            event_sink,
            circuit_breaker,
            clock,
        }
    }

    fn emit_fallback_event(
        &self,
        from: &ClientRoute,
        to: &ClientRoute,
        error: &TauAiError,
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

    fn emit_circuit_opened_event(
        &self,
        route: &ClientRoute,
        route_index: usize,
        opened_until: u64,
    ) {
        let Some(sink) = &self.event_sink else {
            return;
        };
        sink(serde_json::json!({
            "type": "provider_circuit_opened",
            "model": route.model_ref(),
            "route_index": route_index,
            "failure_threshold": self.circuit_breaker.failure_threshold,
            "cooldown_ms": self.circuit_breaker.cooldown_ms,
            "open_until_unix_ms": opened_until,
        }));
    }

    fn emit_circuit_skip_event(&self, route: &ClientRoute, route_index: usize, open_until: u64) {
        let Some(sink) = &self.event_sink else {
            return;
        };
        sink(serde_json::json!({
            "type": "provider_circuit_skip",
            "model": route.model_ref(),
            "route_index": route_index,
            "open_until_unix_ms": open_until,
        }));
    }

    fn route_open_until(&self, route_index: usize, now_unix_ms: u64) -> Option<u64> {
        if !self.circuit_breaker.enabled {
            return None;
        }
        let mut state = lock_or_recover_mutex(&self.route_circuit_state);
        let route_state = state.get_mut(route_index)?;
        let open_until = route_state.open_until_unix_ms?;
        if now_unix_ms < open_until {
            return Some(open_until);
        }
        route_state.open_until_unix_ms = None;
        route_state.consecutive_failures = 0;
        None
    }

    fn record_route_success(&self, route_index: usize) {
        let mut state = lock_or_recover_mutex(&self.route_circuit_state);
        let Some(route_state) = state.get_mut(route_index) else {
            return;
        };
        route_state.open_until_unix_ms = None;
        route_state.consecutive_failures = 0;
    }

    fn record_retryable_route_failure(&self, route_index: usize, now_unix_ms: u64) -> Option<u64> {
        if !self.circuit_breaker.enabled {
            return None;
        }
        let mut state = lock_or_recover_mutex(&self.route_circuit_state);
        let route_state = state.get_mut(route_index)?;
        route_state.consecutive_failures = route_state.consecutive_failures.saturating_add(1);
        let threshold = self.circuit_breaker.failure_threshold.max(1);
        if route_state.consecutive_failures < threshold {
            return None;
        }
        let open_until = now_unix_ms.saturating_add(self.circuit_breaker.cooldown_ms);
        route_state.open_until_unix_ms = Some(open_until);
        route_state.consecutive_failures = 0;
        Some(open_until)
    }

    async fn complete_inner(
        &self,
        request: ChatRequest,
        on_delta: Option<StreamDeltaHandler>,
    ) -> Result<ChatResponse, TauAiError> {
        if self.routes.is_empty() {
            return Err(TauAiError::InvalidResponse(
                "no provider routes configured".to_string(),
            ));
        }

        let mut attempted_route = false;
        for (index, route) in self.routes.iter().enumerate() {
            let now_unix_ms = (self.clock)();
            if let Some(open_until) = self.route_open_until(index, now_unix_ms) {
                self.emit_circuit_skip_event(route, index, open_until);
                continue;
            }
            attempted_route = true;

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
                Ok(response) => {
                    self.record_route_success(index);
                    return Ok(response);
                }
                Err(error) => {
                    if is_retryable_provider_error(&error) {
                        if let Some(open_until) =
                            self.record_retryable_route_failure(index, now_unix_ms)
                        {
                            self.emit_circuit_opened_event(route, index, open_until);
                        }
                        if let Some(next_route) = self.routes.get(index + 1) {
                            self.emit_fallback_event(route, next_route, &error, index + 1);
                            continue;
                        }
                    }
                    return Err(error);
                }
            }
        }

        if !attempted_route {
            return Err(TauAiError::InvalidResponse(
                "all provider routes are temporarily unavailable (circuit breaker open)"
                    .to_string(),
            ));
        }

        Err(TauAiError::InvalidResponse(
            "provider fallback chain exhausted unexpectedly".to_string(),
        ))
    }
}

#[async_trait]
impl LlmClient for FallbackRoutingClient {
    async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, TauAiError> {
        self.complete_inner(request, None).await
    }

    async fn complete_with_stream(
        &self,
        request: ChatRequest,
        on_delta: Option<StreamDeltaHandler>,
    ) -> Result<ChatResponse, TauAiError> {
        self.complete_inner(request, on_delta).await
    }
}

fn is_retryable_status(status: u16) -> bool {
    status == 408 || status == 409 || status == 425 || status == 429 || status >= 500
}

pub fn is_retryable_provider_error(error: &TauAiError) -> bool {
    match error {
        TauAiError::HttpStatus { status, .. } => is_retryable_status(*status),
        TauAiError::Http(inner) => {
            inner.is_timeout() || inner.is_connect() || inner.is_request() || inner.is_body()
        }
        _ => false,
    }
}

fn fallback_error_metadata(error: &TauAiError) -> (&'static str, Option<u16>) {
    match error {
        TauAiError::HttpStatus { status, .. } => ("http_status", Some(*status)),
        TauAiError::Http(inner) if inner.is_timeout() => ("http_timeout", None),
        TauAiError::Http(inner) if inner.is_connect() => ("http_connect", None),
        TauAiError::Http(inner) if inner.is_request() => ("http_request", None),
        TauAiError::Http(inner) if inner.is_body() => ("http_body", None),
        TauAiError::Http(_) => ("http_other", None),
        TauAiError::MissingApiKey => ("missing_api_key", None),
        TauAiError::Serde(_) => ("serde", None),
        TauAiError::InvalidResponse(_) => ("invalid_response", None),
    }
}

fn lock_or_recover_mutex<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

pub fn resolve_fallback_models(cli: &Cli, primary: &ModelRef) -> Result<Vec<ModelRef>> {
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

pub fn build_client_with_fallbacks(
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
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    };

    use serde_json::Value;
    use tau_ai::Message;
    #[derive(Clone)]
    struct MockLlmClient {
        complete_responses: Arc<Mutex<VecDeque<Result<ChatResponse, TauAiError>>>>,
        stream_responses: Arc<Mutex<VecDeque<MockStreamResponse>>>,
        observed_models: Arc<Mutex<Vec<String>>>,
    }

    struct MockStreamResponse {
        deltas: Vec<String>,
        result: Result<ChatResponse, TauAiError>,
    }

    impl MockLlmClient {
        fn new(
            complete_responses: Vec<Result<ChatResponse, TauAiError>>,
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
        async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, TauAiError> {
            self.observed_models
                .lock()
                .expect("observed models lock")
                .push(request.model);

            self.complete_responses
                .lock()
                .expect("complete responses lock")
                .pop_front()
                .unwrap_or_else(|| {
                    Err(TauAiError::InvalidResponse(
                        "no mock complete response configured".to_string(),
                    ))
                })
        }

        async fn complete_with_stream(
            &self,
            request: ChatRequest,
            on_delta: Option<StreamDeltaHandler>,
        ) -> Result<ChatResponse, TauAiError> {
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
                    result: Err(TauAiError::InvalidResponse(
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
            tool_choice: None,
            json_mode: false,
            max_tokens: None,
            temperature: None,
        }
    }

    #[test]
    fn unit_retryable_provider_error_classifies_expected_statuses() {
        assert!(is_retryable_provider_error(&TauAiError::HttpStatus {
            status: 408,
            body: "timeout".to_string(),
        }));
        assert!(is_retryable_provider_error(&TauAiError::HttpStatus {
            status: 409,
            body: "conflict".to_string(),
        }));
        assert!(is_retryable_provider_error(&TauAiError::HttpStatus {
            status: 425,
            body: "too early".to_string(),
        }));
        assert!(is_retryable_provider_error(&TauAiError::HttpStatus {
            status: 429,
            body: "rate limit".to_string(),
        }));
        assert!(is_retryable_provider_error(&TauAiError::HttpStatus {
            status: 500,
            body: "server error".to_string(),
        }));
        assert!(!is_retryable_provider_error(&TauAiError::HttpStatus {
            status: 401,
            body: "unauthorized".to_string(),
        }));
        assert!(!is_retryable_provider_error(&TauAiError::InvalidResponse(
            "bad payload".to_string(),
        )));
    }

    #[test]
    fn unit_circuit_breaker_defaults_are_production_safe() {
        let defaults = CircuitBreakerConfig::default();
        assert!(defaults.enabled);
        assert_eq!(defaults.failure_threshold, 3);
        assert_eq!(defaults.cooldown_ms, 30_000);
    }

    #[tokio::test]
    async fn functional_fallback_client_handoffs_on_retryable_error_and_emits_event() {
        let primary = MockLlmClient::new(
            vec![Err(TauAiError::HttpStatus {
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
                result: Err(TauAiError::HttpStatus {
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
    async fn functional_circuit_breaker_opens_and_skips_temporarily_unhealthy_route() {
        let primary = MockLlmClient::new(
            vec![
                Err(TauAiError::HttpStatus {
                    status: 429,
                    body: "rate limited".to_string(),
                }),
                Err(TauAiError::HttpStatus {
                    status: 429,
                    body: "rate limited".to_string(),
                }),
            ],
            Vec::new(),
        );
        let secondary = MockLlmClient::new(
            vec![
                Ok(assistant_text_response("fallback-1")),
                Ok(assistant_text_response("fallback-2")),
                Ok(assistant_text_response("fallback-3")),
            ],
            Vec::new(),
        );

        let events = Arc::new(Mutex::new(Vec::<Value>::new()));
        let events_sink = events.clone();
        let event_sink: FallbackEventSink =
            Arc::new(move |event| events_sink.lock().expect("events lock").push(event));

        let now_ms = Arc::new(AtomicU64::new(10_000));
        let clock: ClockFn = {
            let now_ms = now_ms.clone();
            Arc::new(move || now_ms.load(Ordering::Relaxed))
        };

        let router = FallbackRoutingClient::new_with_clock(
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
            CircuitBreakerConfig {
                enabled: true,
                failure_threshold: 2,
                cooldown_ms: 5_000,
            },
            clock,
        );

        let _ = router
            .complete(test_request())
            .await
            .expect("first fallback");
        let _ = router
            .complete(test_request())
            .await
            .expect("second fallback");
        let response = router
            .complete(test_request())
            .await
            .expect("third request should skip unhealthy primary");

        assert_eq!(response.message.text_content(), "fallback-3");
        assert_eq!(primary.observed_models().len(), 2);
        assert_eq!(secondary.observed_models().len(), 3);

        let events = events.lock().expect("events lock");
        assert!(
            events
                .iter()
                .any(|event| event["type"] == "provider_circuit_opened"),
            "circuit should open after threshold failures"
        );
        assert!(
            events
                .iter()
                .any(|event| event["type"] == "provider_circuit_skip"),
            "open circuit should skip primary route"
        );
    }

    #[tokio::test]
    async fn integration_circuit_breaker_retries_primary_after_cooldown_expires() {
        let primary = MockLlmClient::new(
            vec![
                Err(TauAiError::HttpStatus {
                    status: 503,
                    body: "unavailable".to_string(),
                }),
                Err(TauAiError::HttpStatus {
                    status: 503,
                    body: "unavailable".to_string(),
                }),
                Ok(assistant_text_response("primary recovered")),
            ],
            Vec::new(),
        );
        let secondary = MockLlmClient::new(
            vec![
                Ok(assistant_text_response("fallback-1")),
                Ok(assistant_text_response("fallback-2")),
            ],
            Vec::new(),
        );

        let now_ms = Arc::new(AtomicU64::new(5_000));
        let clock: ClockFn = {
            let now_ms = now_ms.clone();
            Arc::new(move || now_ms.load(Ordering::Relaxed))
        };

        let router = FallbackRoutingClient::new_with_clock(
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
            CircuitBreakerConfig {
                enabled: true,
                failure_threshold: 2,
                cooldown_ms: 1_000,
            },
            clock,
        );

        let _ = router
            .complete(test_request())
            .await
            .expect("first fallback");
        let _ = router
            .complete(test_request())
            .await
            .expect("second fallback");
        now_ms.store(6_200, Ordering::Relaxed);

        let recovered = router
            .complete(test_request())
            .await
            .expect("primary should be retried after cooldown");
        assert_eq!(recovered.message.text_content(), "primary recovered");
        assert_eq!(primary.observed_models().len(), 3);
        assert_eq!(secondary.observed_models().len(), 2);
    }

    #[tokio::test]
    async fn regression_non_retryable_error_does_not_fallback_to_next_route() {
        let primary = MockLlmClient::new(
            vec![Err(TauAiError::HttpStatus {
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
            TauAiError::HttpStatus { status, body } => {
                assert_eq!(status, 401);
                assert!(body.contains("unauthorized"));
            }
            other => panic!("expected HttpStatus error, got {other:?}"),
        }

        assert_eq!(primary.observed_models(), vec!["gpt-4o-mini".to_string()]);
        assert!(secondary.observed_models().is_empty());
    }

    #[tokio::test]
    async fn regression_non_retryable_error_does_not_trip_circuit_breaker() {
        let primary = MockLlmClient::new(
            vec![
                Err(TauAiError::HttpStatus {
                    status: 401,
                    body: "unauthorized".to_string(),
                }),
                Ok(assistant_text_response("primary-ok")),
            ],
            Vec::new(),
        );
        let secondary = MockLlmClient::new(
            vec![Ok(assistant_text_response("should-not-run"))],
            Vec::new(),
        );
        let events = Arc::new(Mutex::new(Vec::<Value>::new()));
        let events_sink = events.clone();
        let event_sink: FallbackEventSink =
            Arc::new(move |event| events_sink.lock().expect("events lock").push(event));
        let now_ms = Arc::new(AtomicU64::new(1_000));
        let clock: ClockFn = {
            let now_ms = now_ms.clone();
            Arc::new(move || now_ms.load(Ordering::Relaxed))
        };

        let router = FallbackRoutingClient::new_with_clock(
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
            CircuitBreakerConfig {
                enabled: true,
                failure_threshold: 1,
                cooldown_ms: 30_000,
            },
            clock,
        );

        let first_error = router
            .complete(test_request())
            .await
            .expect_err("first call should return non-retryable error");
        assert!(matches!(
            first_error,
            TauAiError::HttpStatus { status: 401, .. }
        ));

        let second = router
            .complete(test_request())
            .await
            .expect("second call should still attempt primary");
        assert_eq!(second.message.text_content(), "primary-ok");
        assert_eq!(primary.observed_models().len(), 2);
        assert!(secondary.observed_models().is_empty());

        let events = events.lock().expect("events lock");
        assert!(
            events
                .iter()
                .all(|event| event["type"] != "provider_circuit_opened"),
            "non-retryable errors must not open the circuit"
        );
    }

    #[tokio::test]
    async fn regression_all_open_circuit_routes_fail_fast_until_cooldown() {
        let primary = MockLlmClient::new(
            vec![Err(TauAiError::HttpStatus {
                status: 503,
                body: "primary unavailable".to_string(),
            })],
            Vec::new(),
        );
        let secondary = MockLlmClient::new(
            vec![Err(TauAiError::HttpStatus {
                status: 503,
                body: "secondary unavailable".to_string(),
            })],
            Vec::new(),
        );
        let now_ms = Arc::new(AtomicU64::new(2_000));
        let clock: ClockFn = {
            let now_ms = now_ms.clone();
            Arc::new(move || now_ms.load(Ordering::Relaxed))
        };

        let router = FallbackRoutingClient::new_with_clock(
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
            CircuitBreakerConfig {
                enabled: true,
                failure_threshold: 1,
                cooldown_ms: 10_000,
            },
            clock,
        );

        let first = router
            .complete(test_request())
            .await
            .expect_err("first run should fail and open both circuits");
        assert!(matches!(first, TauAiError::HttpStatus { status: 503, .. }));

        let second = router
            .complete(test_request())
            .await
            .expect_err("second run should fail fast while both circuits remain open");
        match second {
            TauAiError::InvalidResponse(message) => {
                assert!(message.contains("circuit breaker open"))
            }
            other => panic!("expected circuit-open invalid response, got {other:?}"),
        }

        assert_eq!(primary.observed_models().len(), 1);
        assert_eq!(secondary.observed_models().len(), 1);
    }
}
