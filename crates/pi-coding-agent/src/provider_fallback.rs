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
