//! Gateway auth-session endpoint handler.

use super::*;

pub(super) async fn handle_gateway_auth_session(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    body: Bytes,
) -> Response {
    if state.config.auth_mode != GatewayOpenResponsesAuthMode::PasswordSession {
        return OpenResponsesApiError::bad_request(
            "auth_mode_mismatch",
            "gateway auth session endpoint requires --gateway-openresponses-auth-mode=password-session",
        )
        .into_response();
    }
    if let Err(error) = enforce_gateway_rate_limit(&state, "auth_session_issue") {
        return error.into_response();
    }
    let request = match serde_json::from_slice::<GatewayAuthSessionRequest>(&body) {
        Ok(request) => request,
        Err(error) => {
            return OpenResponsesApiError::bad_request(
                "malformed_json",
                format!("failed to parse request body: {error}"),
            )
            .into_response();
        }
    };

    match issue_gateway_session_token(&state, request.password.as_str()) {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(error) => error.into_response(),
    }
}
