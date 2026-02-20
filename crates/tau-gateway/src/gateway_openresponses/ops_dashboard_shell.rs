use std::sync::Arc;

use axum::extract::{Form, State};
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::Deserialize;
use tau_ai::{Message, MessageRole};
use tau_dashboard_ui::{
    render_tau_ops_dashboard_shell_with_context, TauOpsDashboardAuthMode,
    TauOpsDashboardChatMessageRow, TauOpsDashboardChatSnapshot, TauOpsDashboardRoute,
    TauOpsDashboardShellContext, TauOpsDashboardSidebarState, TauOpsDashboardTheme,
};
use tau_session::SessionStore;

use super::{
    collect_tau_ops_dashboard_command_center_snapshot, gateway_session_path,
    record_cortex_session_append_event, sanitize_session_key, GatewayOpenResponsesServerState,
    OpenResponsesApiError, OpsShellControlsQuery, DEFAULT_SESSION_KEY, OPS_DASHBOARD_CHAT_ENDPOINT,
    OPS_DASHBOARD_CHAT_SEND_ENDPOINT,
};
use crate::remote_profile::GatewayOpenResponsesAuthMode;

pub(super) fn resolve_tau_ops_dashboard_auth_mode(
    mode: GatewayOpenResponsesAuthMode,
) -> TauOpsDashboardAuthMode {
    match mode {
        GatewayOpenResponsesAuthMode::Token => TauOpsDashboardAuthMode::Token,
        GatewayOpenResponsesAuthMode::PasswordSession => TauOpsDashboardAuthMode::PasswordSession,
        GatewayOpenResponsesAuthMode::LocalhostDev => TauOpsDashboardAuthMode::None,
    }
}

#[derive(Debug, Deserialize, Default)]
pub(super) struct OpsDashboardChatSendForm {
    #[serde(default)]
    session_key: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    theme: String,
    #[serde(default)]
    sidebar: String,
}

impl OpsDashboardChatSendForm {
    fn resolved_session_key(&self) -> String {
        let requested = self.session_key.trim();
        let resolved = if requested.is_empty() {
            DEFAULT_SESSION_KEY
        } else {
            requested
        };
        sanitize_session_key(resolved)
    }

    fn resolved_theme(&self) -> TauOpsDashboardTheme {
        match self.theme.trim() {
            "light" => TauOpsDashboardTheme::Light,
            _ => TauOpsDashboardTheme::Dark,
        }
    }

    fn resolved_sidebar_state(&self) -> TauOpsDashboardSidebarState {
        match self.sidebar.trim() {
            "collapsed" => TauOpsDashboardSidebarState::Collapsed,
            _ => TauOpsDashboardSidebarState::Expanded,
        }
    }
}

fn resolve_ops_chat_session_key(controls: &OpsShellControlsQuery) -> String {
    let requested = controls
        .requested_session_key()
        .unwrap_or(DEFAULT_SESSION_KEY);
    sanitize_session_key(requested)
}

fn build_ops_chat_redirect_path(
    theme: TauOpsDashboardTheme,
    sidebar_state: TauOpsDashboardSidebarState,
    session_key: &str,
) -> String {
    format!(
        "{OPS_DASHBOARD_CHAT_ENDPOINT}?theme={}&sidebar={}&session={session_key}",
        theme.as_str(),
        sidebar_state.as_str()
    )
}

fn tau_ops_chat_message_role_label(role: MessageRole) -> &'static str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    }
}

fn collect_tau_ops_dashboard_chat_snapshot(
    state: &Arc<GatewayOpenResponsesServerState>,
    controls: &OpsShellControlsQuery,
) -> TauOpsDashboardChatSnapshot {
    let active_session_key = resolve_ops_chat_session_key(controls);
    let session_path = gateway_session_path(&state.config.state_dir, active_session_key.as_str());
    let mut message_rows = Vec::new();

    if let Ok(store) = SessionStore::load(&session_path) {
        if let Ok(messages) = store.lineage_messages(store.head_id()) {
            for message in messages {
                if matches!(message.role, MessageRole::System) {
                    continue;
                }
                let content = message.text_content();
                if content.trim().is_empty() {
                    continue;
                }
                message_rows.push(TauOpsDashboardChatMessageRow {
                    role: tau_ops_chat_message_role_label(message.role).to_string(),
                    content,
                });
            }
        }
    }

    TauOpsDashboardChatSnapshot {
        active_session_key,
        send_form_action: OPS_DASHBOARD_CHAT_SEND_ENDPOINT.to_string(),
        send_form_method: "post".to_string(),
        message_rows,
    }
}

pub(super) fn render_tau_ops_dashboard_shell_for_route(
    state: &Arc<GatewayOpenResponsesServerState>,
    route: TauOpsDashboardRoute,
    controls: OpsShellControlsQuery,
) -> Html<String> {
    let mut command_center =
        collect_tau_ops_dashboard_command_center_snapshot(&state.config.state_dir);
    command_center.timeline_range = controls.timeline_range().to_string();
    let chat = collect_tau_ops_dashboard_chat_snapshot(state, &controls);

    Html(render_tau_ops_dashboard_shell_with_context(
        TauOpsDashboardShellContext {
            auth_mode: resolve_tau_ops_dashboard_auth_mode(state.config.auth_mode),
            active_route: route,
            theme: controls.theme(),
            sidebar_state: controls.sidebar_state(),
            command_center,
            chat,
        },
    ))
}

pub(super) async fn handle_ops_dashboard_chat_send(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    Form(form): Form<OpsDashboardChatSendForm>,
) -> Response {
    let session_key = form.resolved_session_key();
    let redirect_path = build_ops_chat_redirect_path(
        form.resolved_theme(),
        form.resolved_sidebar_state(),
        session_key.as_str(),
    );
    let content = form.message.trim();
    if content.is_empty() {
        return Redirect::to(redirect_path.as_str()).into_response();
    }

    let session_path = gateway_session_path(&state.config.state_dir, session_key.as_str());
    let mut store = match SessionStore::load(&session_path) {
        Ok(store) => store,
        Err(error) => {
            return OpenResponsesApiError::internal(format!(
                "failed to load session '{}': {error}",
                session_path.display()
            ))
            .into_response();
        }
    };
    store.set_lock_policy(
        state.config.session_lock_wait_ms,
        state.config.session_lock_stale_ms,
    );

    let resolved_system_prompt = state.resolved_system_prompt();
    if let Err(error) = store.ensure_initialized(&resolved_system_prompt) {
        return OpenResponsesApiError::internal(format!(
            "failed to initialize session '{}': {error}",
            session_path.display()
        ))
        .into_response();
    }

    let parent_id = store.head_id();
    let message = Message::user(content);
    let new_head = match store.append_messages(parent_id, &[message]) {
        Ok(head) => head,
        Err(error) => {
            return OpenResponsesApiError::internal(format!(
                "failed to append session message '{}': {error}",
                session_path.display()
            ))
            .into_response();
        }
    };

    state.record_ui_telemetry_event("chat", "send", "chat_message_appended");
    record_cortex_session_append_event(
        &state.config.state_dir,
        session_key.as_str(),
        new_head,
        store.entries().len(),
    );
    Redirect::to(redirect_path.as_str()).into_response()
}
