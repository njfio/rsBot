//! Gateway shell and auth entry route handlers.

use super::dashboard_shell_page::render_gateway_dashboard_shell_page;
use super::ops_dashboard_shell::resolve_tau_ops_dashboard_auth_mode;
use super::types::GatewayAuthBootstrapResponse;
use super::webchat_page::render_gateway_webchat_page;
use super::*;

pub(super) async fn handle_webchat_page() -> Html<String> {
    Html(render_gateway_webchat_page())
}

pub(super) async fn handle_dashboard_shell_page() -> Html<String> {
    Html(render_gateway_dashboard_shell_page())
}

pub(super) async fn handle_gateway_auth_bootstrap(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
) -> Response {
    if let Err(error) = enforce_gateway_rate_limit(&state, "gateway_auth_bootstrap") {
        return error.into_response();
    }

    let auth_mode = resolve_tau_ops_dashboard_auth_mode(state.config.auth_mode);
    (
        StatusCode::OK,
        Json(GatewayAuthBootstrapResponse {
            auth_mode: state.config.auth_mode.as_str().to_string(),
            ui_auth_mode: auth_mode.as_str().to_string(),
            requires_authentication: auth_mode.requires_authentication(),
            ops_endpoint: OPS_DASHBOARD_ENDPOINT,
            ops_login_endpoint: OPS_DASHBOARD_LOGIN_ENDPOINT,
            auth_session_endpoint: GATEWAY_AUTH_SESSION_ENDPOINT,
        }),
    )
        .into_response()
}
