//! AuthN/AuthZ and rate-limit runtime helpers for gateway OpenResponses.
use super::*;

#[derive(Debug, Clone, Default)]
pub(super) struct GatewayAuthRuntimeState {
    pub(super) sessions: BTreeMap<String, GatewaySessionTokenState>,
    pub(super) total_sessions_issued: u64,
    pub(super) auth_failures: u64,
    pub(super) rate_limited_requests: u64,
    pub(super) rate_limit_buckets: BTreeMap<String, GatewayRateLimitBucket>,
}

#[derive(Debug, Clone)]
pub(super) struct GatewaySessionTokenState {
    pub(super) expires_unix_ms: u64,
    pub(super) last_seen_unix_ms: u64,
    pub(super) request_count: u64,
}

#[derive(Debug, Clone, Default)]
pub(super) struct GatewayRateLimitBucket {
    pub(super) window_started_unix_ms: u64,
    pub(super) accepted_requests: usize,
    pub(super) rejected_requests: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(super) struct GatewayAuthStatusReport {
    mode: String,
    session_ttl_seconds: u64,
    active_sessions: usize,
    total_sessions_issued: u64,
    auth_failures: u64,
    rate_limited_requests: u64,
    rate_limit_window_seconds: u64,
    rate_limit_max_requests: usize,
}

fn bearer_token_from_headers(headers: &HeaderMap) -> Option<String> {
    let header = headers.get(AUTHORIZATION)?;
    let raw = header.to_str().ok()?;
    let token = raw.strip_prefix("Bearer ")?;
    let token = token.trim();
    if token.is_empty() {
        return None;
    }
    Some(token.to_string())
}

fn note_gateway_auth_failure(state: &GatewayOpenResponsesServerState) {
    if let Ok(mut auth_state) = state.auth_runtime.lock() {
        auth_state.auth_failures = auth_state.auth_failures.saturating_add(1);
    }
}

pub(super) fn prune_expired_gateway_sessions(
    auth_state: &mut GatewayAuthRuntimeState,
    now_unix_ms: u64,
) {
    auth_state
        .sessions
        .retain(|_, session| session.expires_unix_ms > now_unix_ms);
}

pub(super) fn authorize_gateway_request(
    state: &GatewayOpenResponsesServerState,
    headers: &HeaderMap,
) -> Result<String, OpenResponsesApiError> {
    match state.config.auth_mode {
        GatewayOpenResponsesAuthMode::LocalhostDev => Ok("localhost-dev".to_string()),
        GatewayOpenResponsesAuthMode::Token => {
            let expected = state
                .config
                .auth_token
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    OpenResponsesApiError::internal("gateway token auth mode is misconfigured")
                })?;
            let Some(observed) = bearer_token_from_headers(headers) else {
                note_gateway_auth_failure(state);
                return Err(OpenResponsesApiError::unauthorized());
            };
            if observed != expected {
                note_gateway_auth_failure(state);
                return Err(OpenResponsesApiError::unauthorized());
            }
            Ok("token".to_string())
        }
        GatewayOpenResponsesAuthMode::PasswordSession => {
            let Some(session_token) = bearer_token_from_headers(headers) else {
                note_gateway_auth_failure(state);
                return Err(OpenResponsesApiError::unauthorized());
            };
            let now_unix_ms = current_unix_timestamp_ms();
            let mut auth_state = state
                .auth_runtime
                .lock()
                .map_err(|_| OpenResponsesApiError::internal("gateway auth state lock poisoned"))?;
            prune_expired_gateway_sessions(&mut auth_state, now_unix_ms);
            if let Some(session) = auth_state.sessions.get_mut(session_token.as_str()) {
                session.last_seen_unix_ms = now_unix_ms;
                session.request_count = session.request_count.saturating_add(1);
                return Ok(format!("session:{session_token}"));
            }
            auth_state.auth_failures = auth_state.auth_failures.saturating_add(1);
            Err(OpenResponsesApiError::unauthorized())
        }
    }
}

pub(super) fn enforce_gateway_rate_limit(
    state: &GatewayOpenResponsesServerState,
    principal: &str,
) -> Result<(), OpenResponsesApiError> {
    let window_ms = state
        .config
        .rate_limit_window_seconds
        .saturating_mul(1000)
        .max(1);
    let max_requests = state.config.rate_limit_max_requests.max(1);
    let now_unix_ms = current_unix_timestamp_ms();
    let mut auth_state = state
        .auth_runtime
        .lock()
        .map_err(|_| OpenResponsesApiError::internal("gateway auth state lock poisoned"))?;

    let bucket = auth_state
        .rate_limit_buckets
        .entry(principal.to_string())
        .or_default();
    if bucket.window_started_unix_ms == 0
        || now_unix_ms.saturating_sub(bucket.window_started_unix_ms) >= window_ms
    {
        bucket.window_started_unix_ms = now_unix_ms;
        bucket.accepted_requests = 0;
        bucket.rejected_requests = 0;
    }
    if bucket.accepted_requests >= max_requests {
        bucket.rejected_requests = bucket.rejected_requests.saturating_add(1);
        auth_state.rate_limited_requests = auth_state.rate_limited_requests.saturating_add(1);
        return Err(OpenResponsesApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            format!(
                "gateway rate limit exceeded: max {} requests per {} seconds",
                max_requests, state.config.rate_limit_window_seconds
            ),
        ));
    }
    bucket.accepted_requests = bucket.accepted_requests.saturating_add(1);
    Ok(())
}

pub(super) fn issue_gateway_session_token(
    state: &GatewayOpenResponsesServerState,
    password: &str,
) -> Result<GatewayAuthSessionResponse, OpenResponsesApiError> {
    let expected_password = state
        .config
        .auth_password
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| OpenResponsesApiError::internal("gateway password auth is misconfigured"))?;
    if password.trim().is_empty() || password.trim() != expected_password {
        note_gateway_auth_failure(state);
        return Err(OpenResponsesApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            "invalid gateway password",
        ));
    }

    let now_unix_ms = current_unix_timestamp_ms();
    let ttl_ms = state
        .config
        .session_ttl_seconds
        .saturating_mul(1000)
        .max(1000);
    let expires_unix_ms = now_unix_ms.saturating_add(ttl_ms);
    let access_token = format!("tau_sess_{:016x}", state.next_sequence());
    let mut auth_state = state
        .auth_runtime
        .lock()
        .map_err(|_| OpenResponsesApiError::internal("gateway auth state lock poisoned"))?;
    prune_expired_gateway_sessions(&mut auth_state, now_unix_ms);
    auth_state.sessions.insert(
        access_token.clone(),
        GatewaySessionTokenState {
            expires_unix_ms,
            last_seen_unix_ms: now_unix_ms,
            request_count: 0,
        },
    );
    auth_state.total_sessions_issued = auth_state.total_sessions_issued.saturating_add(1);
    Ok(GatewayAuthSessionResponse {
        access_token,
        token_type: "bearer",
        expires_unix_ms,
        expires_in_seconds: state.config.session_ttl_seconds,
    })
}

pub(super) fn collect_gateway_auth_status_report(
    state: &GatewayOpenResponsesServerState,
) -> GatewayAuthStatusReport {
    let mut active_sessions = 0usize;
    let mut total_sessions_issued = 0u64;
    let mut auth_failures = 0u64;
    let mut rate_limited_requests = 0u64;
    if let Ok(mut auth_state) = state.auth_runtime.lock() {
        prune_expired_gateway_sessions(&mut auth_state, current_unix_timestamp_ms());
        active_sessions = auth_state.sessions.len();
        total_sessions_issued = auth_state.total_sessions_issued;
        auth_failures = auth_state.auth_failures;
        rate_limited_requests = auth_state.rate_limited_requests;
    }
    GatewayAuthStatusReport {
        mode: state.config.auth_mode.as_str().to_string(),
        session_ttl_seconds: state.config.session_ttl_seconds,
        active_sessions,
        total_sessions_issued,
        auth_failures,
        rate_limited_requests,
        rate_limit_window_seconds: state.config.rate_limit_window_seconds,
        rate_limit_max_requests: state.config.rate_limit_max_requests,
    }
}
