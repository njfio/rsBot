use std::collections::BTreeSet;
use std::sync::Arc;

use axum::extract::{Form, State};
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::Deserialize;
use tau_ai::{Message, MessageRole};
use tau_dashboard_ui::{
    render_tau_ops_dashboard_shell_with_context, TauOpsDashboardAuthMode,
    TauOpsDashboardChatMessageRow, TauOpsDashboardChatSessionOptionRow,
    TauOpsDashboardChatSnapshot, TauOpsDashboardRoute, TauOpsDashboardSessionGraphEdgeRow,
    TauOpsDashboardSessionGraphNodeRow, TauOpsDashboardSessionTimelineRow,
    TauOpsDashboardShellContext, TauOpsDashboardSidebarState, TauOpsDashboardTheme,
};
use tau_session::SessionStore;

use super::{
    collect_tau_ops_dashboard_command_center_snapshot, gateway_session_path,
    record_cortex_session_append_event, sanitize_session_key, GatewayOpenResponsesServerState,
    OpenResponsesApiError, OpsShellControlsQuery, DEFAULT_SESSION_KEY, OPS_DASHBOARD_CHAT_ENDPOINT,
    OPS_DASHBOARD_CHAT_NEW_ENDPOINT, OPS_DASHBOARD_CHAT_SEND_ENDPOINT,
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

fn resolve_chat_theme(theme: &str) -> TauOpsDashboardTheme {
    match theme.trim() {
        "light" => TauOpsDashboardTheme::Light,
        _ => TauOpsDashboardTheme::Dark,
    }
}

fn resolve_chat_sidebar_state(sidebar: &str) -> TauOpsDashboardSidebarState {
    match sidebar.trim() {
        "collapsed" => TauOpsDashboardSidebarState::Collapsed,
        _ => TauOpsDashboardSidebarState::Expanded,
    }
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
        resolve_chat_theme(self.theme.as_str())
    }

    fn resolved_sidebar_state(&self) -> TauOpsDashboardSidebarState {
        resolve_chat_sidebar_state(self.sidebar.as_str())
    }
}

#[derive(Debug, Deserialize, Default)]
pub(super) struct OpsDashboardChatNewSessionForm {
    #[serde(default)]
    session_key: String,
    #[serde(default)]
    theme: String,
    #[serde(default)]
    sidebar: String,
}

impl OpsDashboardChatNewSessionForm {
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
        resolve_chat_theme(self.theme.as_str())
    }

    fn resolved_sidebar_state(&self) -> TauOpsDashboardSidebarState {
        resolve_chat_sidebar_state(self.sidebar.as_str())
    }
}

fn resolve_ops_chat_session_key(
    controls: &OpsShellControlsQuery,
    detail_session_key: Option<&str>,
) -> String {
    if let Some(detail_session_key) = detail_session_key {
        let sanitized = sanitize_session_key(detail_session_key);
        if !sanitized.is_empty() {
            return sanitized;
        }
    }
    let requested = controls
        .requested_session_key()
        .unwrap_or(DEFAULT_SESSION_KEY);
    sanitize_session_key(requested)
}

fn collect_ops_chat_session_option_rows(
    state: &Arc<GatewayOpenResponsesServerState>,
    active_session_key: &str,
) -> Vec<TauOpsDashboardChatSessionOptionRow> {
    let mut session_keys = BTreeSet::new();

    let sessions_root = state
        .config
        .state_dir
        .join("openresponses")
        .join("sessions");
    if sessions_root.is_dir() {
        if let Ok(dir_entries) = std::fs::read_dir(&sessions_root) {
            for dir_entry in dir_entries.flatten() {
                let path = dir_entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
                    continue;
                }
                let Some(file_stem) = path.file_stem().and_then(|value| value.to_str()) else {
                    continue;
                };
                let session_key = sanitize_session_key(file_stem);
                if session_key.is_empty() {
                    continue;
                }
                session_keys.insert(session_key);
            }
        }
    }

    session_keys
        .into_iter()
        .map(|session_key| TauOpsDashboardChatSessionOptionRow {
            selected: session_key == active_session_key,
            session_key,
        })
        .collect()
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
    detail_session_key: Option<&str>,
) -> TauOpsDashboardChatSnapshot {
    let active_session_key = resolve_ops_chat_session_key(controls, detail_session_key);
    let session_options = collect_ops_chat_session_option_rows(state, active_session_key.as_str());
    let session_path = gateway_session_path(&state.config.state_dir, active_session_key.as_str());
    let mut message_rows = Vec::new();
    let mut session_detail_validation_entries: usize = 0;
    let mut session_detail_validation_duplicates: usize = 0;
    let mut session_detail_validation_invalid_parent: usize = 0;
    let mut session_detail_validation_cycles: usize = 0;
    let mut session_detail_validation_is_valid = true;
    let mut session_detail_usage_input_tokens: u64 = 0;
    let mut session_detail_usage_output_tokens: u64 = 0;
    let mut session_detail_usage_total_tokens: u64 = 0;
    let mut session_detail_usage_estimated_cost_usd = "0.000000".to_string();
    let mut session_detail_timeline_rows = Vec::new();
    let mut session_graph_node_rows = Vec::new();
    let mut session_graph_edge_rows = Vec::new();

    if let Ok(store) = SessionStore::load(&session_path) {
        let validation = store.validation_report();
        session_detail_validation_entries = validation.entries;
        session_detail_validation_duplicates = validation.duplicates;
        session_detail_validation_invalid_parent = validation.invalid_parent;
        session_detail_validation_cycles = validation.cycles;
        session_detail_validation_is_valid = validation.is_valid();

        let usage = store.usage_summary();
        session_detail_usage_input_tokens = usage.input_tokens;
        session_detail_usage_output_tokens = usage.output_tokens;
        session_detail_usage_total_tokens = usage.total_tokens;
        session_detail_usage_estimated_cost_usd = format!("{:.6}", usage.estimated_cost_usd);

        if let Ok(lineage_entries) = store.lineage_entries(store.head_id()) {
            for entry in lineage_entries {
                let role = tau_ops_chat_message_role_label(entry.message.role).to_string();
                session_graph_node_rows.push(TauOpsDashboardSessionGraphNodeRow {
                    entry_id: entry.id,
                    role: role.clone(),
                });
                if let Some(parent_id) = entry.parent_id {
                    session_graph_edge_rows.push(TauOpsDashboardSessionGraphEdgeRow {
                        source_entry_id: parent_id,
                        target_entry_id: entry.id,
                    });
                }

                let content = entry.message.text_content();
                if content.trim().is_empty() {
                    continue;
                }
                session_detail_timeline_rows.push(TauOpsDashboardSessionTimelineRow {
                    entry_id: entry.id,
                    role: role.clone(),
                    content: content.clone(),
                });
                if matches!(entry.message.role, MessageRole::System) {
                    continue;
                }
                message_rows.push(TauOpsDashboardChatMessageRow { role, content });
            }
        }
    }

    TauOpsDashboardChatSnapshot {
        active_session_key: active_session_key.clone(),
        new_session_form_action: OPS_DASHBOARD_CHAT_NEW_ENDPOINT.to_string(),
        new_session_form_method: "post".to_string(),
        send_form_action: OPS_DASHBOARD_CHAT_SEND_ENDPOINT.to_string(),
        send_form_method: "post".to_string(),
        session_options,
        message_rows,
        session_detail_visible: detail_session_key.is_some(),
        session_detail_route: format!("/ops/sessions/{active_session_key}"),
        session_detail_validation_entries,
        session_detail_validation_duplicates,
        session_detail_validation_invalid_parent,
        session_detail_validation_cycles,
        session_detail_validation_is_valid,
        session_detail_usage_input_tokens,
        session_detail_usage_output_tokens,
        session_detail_usage_total_tokens,
        session_detail_usage_estimated_cost_usd,
        session_detail_timeline_rows,
        session_graph_node_rows,
        session_graph_edge_rows,
    }
}

pub(super) fn render_tau_ops_dashboard_shell_for_route(
    state: &Arc<GatewayOpenResponsesServerState>,
    route: TauOpsDashboardRoute,
    controls: OpsShellControlsQuery,
    detail_session_key: Option<&str>,
) -> Html<String> {
    let mut command_center =
        collect_tau_ops_dashboard_command_center_snapshot(&state.config.state_dir);
    command_center.timeline_range = controls.timeline_range().to_string();
    let chat = collect_tau_ops_dashboard_chat_snapshot(state, &controls, detail_session_key);

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

pub(super) async fn handle_ops_dashboard_chat_new(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    Form(form): Form<OpsDashboardChatNewSessionForm>,
) -> Response {
    let session_key = form.resolved_session_key();
    let redirect_path = build_ops_chat_redirect_path(
        form.resolved_theme(),
        form.resolved_sidebar_state(),
        session_key.as_str(),
    );

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

    state.record_ui_telemetry_event("chat", "new-session", "chat_session_initialized");
    Redirect::to(redirect_path.as_str()).into_response()
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
    let content = form.message.as_str();
    if content.trim().is_empty() {
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
