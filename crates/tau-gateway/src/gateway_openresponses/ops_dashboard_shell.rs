use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use axum::extract::{Form, Path as AxumPath, State};
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::Deserialize;
use tau_ai::{Message, MessageRole};
use tau_dashboard_ui::{
    render_tau_ops_dashboard_shell_with_context, TauOpsDashboardAuthMode,
    TauOpsDashboardChatMessageRow, TauOpsDashboardChatSessionOptionRow,
    TauOpsDashboardChatSnapshot, TauOpsDashboardMemoryRelationRow, TauOpsDashboardMemorySearchRow,
    TauOpsDashboardRoute, TauOpsDashboardSessionGraphEdgeRow, TauOpsDashboardSessionGraphNodeRow,
    TauOpsDashboardSessionTimelineRow, TauOpsDashboardShellContext, TauOpsDashboardSidebarState,
    TauOpsDashboardTheme,
};
use tau_memory::memory_contract::{MemoryEntry, MemoryScope};
use tau_memory::runtime::{
    MemoryRelationInput, MemoryScopeFilter, MemorySearchOptions, MemoryType,
};
use tau_session::SessionStore;

use super::{
    collect_tau_ops_dashboard_command_center_snapshot, gateway_memory_store, gateway_session_path,
    record_cortex_memory_entry_delete_event, record_cortex_memory_entry_write_event,
    record_cortex_session_append_event, record_cortex_session_reset_event, sanitize_session_key,
    GatewayOpenResponsesServerState, OpenResponsesApiError, OpsShellControlsQuery,
    DEFAULT_SESSION_KEY, OPS_DASHBOARD_CHAT_ENDPOINT, OPS_DASHBOARD_CHAT_NEW_ENDPOINT,
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

#[derive(Debug, Deserialize, Default)]
pub(super) struct OpsDashboardMemoryCreateForm {
    #[serde(default)]
    session: String,
    #[serde(default)]
    operation: String,
    #[serde(default)]
    entry_id: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    tags: String,
    #[serde(default)]
    facts: String,
    #[serde(default)]
    source_event_key: String,
    #[serde(default)]
    workspace_id: String,
    #[serde(default)]
    channel_id: String,
    #[serde(default)]
    actor_id: String,
    #[serde(default)]
    memory_type: String,
    #[serde(default)]
    importance: String,
    #[serde(default)]
    relation_target_id: String,
    #[serde(default)]
    relation_type: String,
    #[serde(default)]
    relation_weight: String,
    #[serde(default)]
    confirm_delete: String,
    #[serde(default)]
    theme: String,
    #[serde(default)]
    sidebar: String,
}

fn split_memory_form_list(input: &str) -> Vec<String> {
    input
        .split([',', '|', '\n'])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn normalize_memory_form_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

impl OpsDashboardMemoryCreateForm {
    fn is_edit_operation(&self) -> bool {
        self.operation.trim() == "edit"
    }

    fn is_delete_operation(&self) -> bool {
        self.operation.trim() == "delete"
    }

    fn is_delete_confirmed(&self) -> bool {
        self.confirm_delete.trim() == "true"
    }

    fn resolved_session_key(&self) -> String {
        let requested = self.session.trim();
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

    fn resolved_entry_id(&self) -> String {
        let requested = self.entry_id.trim();
        if requested.is_empty() {
            String::new()
        } else {
            sanitize_session_key(requested)
        }
    }

    fn resolved_summary(&self) -> String {
        self.summary.trim().to_string()
    }

    fn resolved_tags(&self) -> Vec<String> {
        split_memory_form_list(self.tags.as_str())
    }

    fn resolved_facts(&self) -> Vec<String> {
        split_memory_form_list(self.facts.as_str())
    }

    fn resolved_source_event_key(&self, entry_id: &str) -> String {
        normalize_memory_form_text(self.source_event_key.as_str())
            .unwrap_or_else(|| format!("ops-memory-create-{entry_id}"))
    }

    fn resolved_workspace_id(&self, session_key: &str) -> String {
        normalize_memory_form_text(self.workspace_id.as_str())
            .unwrap_or_else(|| session_key.to_string())
    }

    fn resolved_channel_id(&self) -> String {
        normalize_memory_form_text(self.channel_id.as_str())
            .unwrap_or_else(|| "gateway".to_string())
    }

    fn resolved_actor_id(&self) -> String {
        normalize_memory_form_text(self.actor_id.as_str()).unwrap_or_else(|| "operator".to_string())
    }

    fn resolved_memory_type(&self) -> Option<MemoryType> {
        normalize_memory_form_text(self.memory_type.as_str())
            .and_then(|memory_type| MemoryType::parse(memory_type.as_str()))
    }

    fn resolved_importance(&self) -> Option<f32> {
        self.importance
            .trim()
            .parse::<f32>()
            .ok()
            .map(|value| value.clamp(0.0, 1.0))
    }

    fn resolved_relations(&self) -> Vec<MemoryRelationInput> {
        let Some(target_id) = normalize_memory_form_text(self.relation_target_id.as_str()) else {
            return Vec::new();
        };

        vec![MemoryRelationInput {
            target_id,
            relation_type: normalize_memory_form_text(self.relation_type.as_str()),
            weight: self.relation_weight.trim().parse::<f32>().ok(),
        }]
    }
}

#[derive(Debug, Deserialize, Default)]
pub(super) struct OpsDashboardSessionBranchForm {
    #[serde(default)]
    source_session_key: String,
    #[serde(default)]
    entry_id: String,
    #[serde(default)]
    target_session_key: String,
    #[serde(default)]
    theme: String,
    #[serde(default)]
    sidebar: String,
}

impl OpsDashboardSessionBranchForm {
    fn resolved_source_session_key(&self) -> String {
        let requested = self.source_session_key.trim();
        let resolved = if requested.is_empty() {
            DEFAULT_SESSION_KEY
        } else {
            requested
        };
        sanitize_session_key(resolved)
    }

    fn resolved_target_session_key(
        &self,
        source_session_key: &str,
        entry_id: Option<u64>,
    ) -> String {
        let requested = self.target_session_key.trim();
        if !requested.is_empty() {
            return sanitize_session_key(requested);
        }
        let fallback = match entry_id {
            Some(entry_id) => format!("{source_session_key}-branch-{entry_id}"),
            None => format!("{source_session_key}-branch"),
        };
        sanitize_session_key(fallback.as_str())
    }

    fn resolved_entry_id(&self) -> Option<u64> {
        let requested = self.entry_id.trim();
        if requested.is_empty() {
            return None;
        }
        requested.parse::<u64>().ok()
    }

    fn resolved_theme(&self) -> TauOpsDashboardTheme {
        resolve_chat_theme(self.theme.as_str())
    }

    fn resolved_sidebar_state(&self) -> TauOpsDashboardSidebarState {
        resolve_chat_sidebar_state(self.sidebar.as_str())
    }
}

#[derive(Debug, Deserialize, Default)]
pub(super) struct OpsDashboardSessionResetForm {
    #[serde(default)]
    session_key: String,
    #[serde(default)]
    confirm_reset: String,
    #[serde(default)]
    theme: String,
    #[serde(default)]
    sidebar: String,
}

impl OpsDashboardSessionResetForm {
    fn resolved_session_key(&self, route_session_key: &str) -> String {
        let requested = self.session_key.trim();
        let resolved = if requested.is_empty() {
            route_session_key
        } else {
            requested
        };
        sanitize_session_key(resolved)
    }

    fn is_confirmed(&self) -> bool {
        self.confirm_reset.trim() == "true"
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
        .map(|session_key| {
            let session_path = gateway_session_path(&state.config.state_dir, session_key.as_str());
            let updated_unix_ms = std::fs::metadata(&session_path)
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
                .unwrap_or(0);
            let (entry_count, usage_total_tokens, validation_is_valid) =
                match SessionStore::load(&session_path) {
                    Ok(store) => {
                        let validation = store.validation_report();
                        let usage = store.usage_summary();
                        (
                            validation.entries,
                            usage.total_tokens,
                            validation.is_valid(),
                        )
                    }
                    Err(_) => (0, 0, false),
                };

            TauOpsDashboardChatSessionOptionRow {
                selected: session_key == active_session_key,
                session_key,
                entry_count,
                usage_total_tokens,
                validation_is_valid,
                updated_unix_ms,
            }
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

fn build_ops_session_detail_redirect_path(
    theme: TauOpsDashboardTheme,
    sidebar_state: TauOpsDashboardSidebarState,
    session_key: &str,
) -> String {
    format!(
        "/ops/sessions/{session_key}?theme={}&sidebar={}",
        theme.as_str(),
        sidebar_state.as_str()
    )
}

fn build_ops_memory_redirect_path(
    theme: TauOpsDashboardTheme,
    sidebar_state: TauOpsDashboardSidebarState,
    session_key: &str,
    create_status: &str,
    created_memory_id: Option<&str>,
    delete_status: &str,
    deleted_memory_id: Option<&str>,
) -> String {
    let mut redirect_path = format!(
        "/ops/memory?theme={}&sidebar={}&session={session_key}&create_status={create_status}&delete_status={delete_status}",
        theme.as_str(),
        sidebar_state.as_str()
    );
    if let Some(memory_id) = created_memory_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        redirect_path.push_str("&created_memory_id=");
        redirect_path.push_str(memory_id);
    }
    if let Some(memory_id) = deleted_memory_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        redirect_path.push_str("&deleted_memory_id=");
        redirect_path.push_str(memory_id);
    }
    redirect_path
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
    let memory_search_query = controls
        .requested_memory_query()
        .map(str::to_string)
        .unwrap_or_default();
    let memory_search_workspace_id = controls.requested_memory_workspace_id().unwrap_or_default();
    let memory_search_channel_id = controls.requested_memory_channel_id().unwrap_or_default();
    let memory_search_actor_id = controls.requested_memory_actor_id().unwrap_or_default();
    let memory_search_memory_type = controls.requested_memory_type().unwrap_or_default();
    let memory_create_workspace_id = memory_search_workspace_id.clone();
    let memory_create_channel_id = memory_search_channel_id.clone();
    let memory_create_actor_id = memory_search_actor_id.clone();
    let memory_create_memory_type = memory_search_memory_type.clone();
    let memory_create_status = controls.requested_memory_create_status().to_string();
    let memory_create_created_entry_id = controls
        .requested_memory_created_entry_id()
        .unwrap_or_default();
    let memory_delete_status = controls.requested_memory_delete_status().to_string();
    let memory_delete_deleted_entry_id = controls
        .requested_memory_deleted_entry_id()
        .unwrap_or_default();
    let mut memory_search_rows = Vec::new();
    let mut memory_detail_visible = false;
    let mut memory_detail_selected_entry_id = controls
        .requested_memory_detail_entry_id()
        .unwrap_or_default();
    let mut memory_detail_summary = String::new();
    let mut memory_detail_memory_type = String::new();
    let mut memory_detail_embedding_source = String::new();
    let mut memory_detail_embedding_model = String::new();
    let mut memory_detail_embedding_reason_code = String::new();
    let mut memory_detail_embedding_dimensions = 0usize;
    let mut memory_detail_relation_rows = Vec::new();
    let store = gateway_memory_store(&state.config.state_dir, active_session_key.as_str());

    if !memory_search_query.trim().is_empty() {
        let search_options = MemorySearchOptions {
            limit: controls.requested_memory_limit(),
            scope: MemoryScopeFilter {
                workspace_id: (!memory_search_workspace_id.is_empty())
                    .then_some(memory_search_workspace_id.clone()),
                channel_id: (!memory_search_channel_id.is_empty())
                    .then_some(memory_search_channel_id.clone()),
                actor_id: (!memory_search_actor_id.is_empty())
                    .then_some(memory_search_actor_id.clone()),
            },
            ..MemorySearchOptions::default()
        };
        if let Ok(search_result) = store.search(memory_search_query.as_str(), &search_options) {
            memory_search_rows = search_result
                .matches
                .iter()
                .filter(|entry| {
                    memory_search_memory_type.is_empty()
                        || entry.memory_type.as_str() == memory_search_memory_type.as_str()
                })
                .map(|entry| TauOpsDashboardMemorySearchRow {
                    memory_id: entry.memory_id.clone(),
                    summary: entry.summary.clone(),
                    memory_type: entry.memory_type.as_str().to_string(),
                    score: format!("{:.4}", entry.score),
                })
                .take(search_options.limit)
                .collect();
        }
    }

    if !memory_detail_selected_entry_id.trim().is_empty() {
        match store.read_entry(memory_detail_selected_entry_id.as_str(), None) {
            Ok(Some(record)) => {
                memory_detail_visible = true;
                memory_detail_summary = record.entry.summary.clone();
                memory_detail_memory_type = record.memory_type.as_str().to_string();
                memory_detail_embedding_source = record.embedding_source.clone();
                memory_detail_embedding_model = record.embedding_model.clone().unwrap_or_default();
                memory_detail_embedding_reason_code = record.embedding_reason_code.clone();
                memory_detail_embedding_dimensions = record.embedding_vector.len();
                memory_detail_relation_rows = record
                    .relations
                    .iter()
                    .map(|relation| TauOpsDashboardMemoryRelationRow {
                        target_id: relation.target_id.clone(),
                        relation_type: relation.relation_type.as_str().to_string(),
                        effective_weight: format!("{:.4}", relation.effective_weight),
                    })
                    .collect();
            }
            Ok(None) | Err(_) => {
                memory_detail_selected_entry_id.clear();
            }
        }
    }

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
        memory_search_form_action: "/ops/memory".to_string(),
        memory_search_form_method: "get".to_string(),
        memory_search_query,
        memory_search_workspace_id,
        memory_search_channel_id,
        memory_search_actor_id,
        memory_search_memory_type,
        memory_search_rows,
        memory_create_form_action: "/ops/memory".to_string(),
        memory_create_form_method: "post".to_string(),
        memory_create_status,
        memory_create_created_entry_id,
        memory_create_entry_id: String::new(),
        memory_create_summary: String::new(),
        memory_create_tags: String::new(),
        memory_create_facts: String::new(),
        memory_create_source_event_key: String::new(),
        memory_create_workspace_id,
        memory_create_channel_id,
        memory_create_actor_id,
        memory_create_memory_type,
        memory_create_importance: String::new(),
        memory_create_relation_target_id: String::new(),
        memory_create_relation_type: String::new(),
        memory_create_relation_weight: String::new(),
        memory_delete_status,
        memory_delete_deleted_entry_id,
        memory_detail_visible,
        memory_detail_selected_entry_id,
        memory_detail_summary,
        memory_detail_memory_type,
        memory_detail_embedding_source,
        memory_detail_embedding_model,
        memory_detail_embedding_reason_code,
        memory_detail_embedding_dimensions,
        memory_detail_relation_rows,
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

pub(super) async fn handle_ops_dashboard_memory_create(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    Form(form): Form<OpsDashboardMemoryCreateForm>,
) -> Response {
    let session_key = form.resolved_session_key();
    let is_edit_operation = form.is_edit_operation();
    let is_delete_operation = form.is_delete_operation();
    let fallback_redirect_path = build_ops_memory_redirect_path(
        form.resolved_theme(),
        form.resolved_sidebar_state(),
        session_key.as_str(),
        "idle",
        None,
        "idle",
        None,
    );

    let entry_id = form.resolved_entry_id();
    if is_delete_operation {
        if entry_id.is_empty() || !form.is_delete_confirmed() {
            return Redirect::to(fallback_redirect_path.as_str()).into_response();
        }
        let store = gateway_memory_store(&state.config.state_dir, session_key.as_str());
        match store.soft_delete_entry(entry_id.as_str(), None) {
            Ok(Some(_)) => {
                state.record_ui_telemetry_event(
                    "memory",
                    "entry_delete",
                    "ops_memory_delete_form_submitted",
                );
                record_cortex_memory_entry_delete_event(
                    &state.config.state_dir,
                    session_key.as_str(),
                    entry_id.as_str(),
                    true,
                );
                let redirect_path = build_ops_memory_redirect_path(
                    form.resolved_theme(),
                    form.resolved_sidebar_state(),
                    session_key.as_str(),
                    "idle",
                    None,
                    "deleted",
                    Some(entry_id.as_str()),
                );
                return Redirect::to(redirect_path.as_str()).into_response();
            }
            Ok(None) => return Redirect::to(fallback_redirect_path.as_str()).into_response(),
            Err(error) => {
                return OpenResponsesApiError::internal(format!(
                    "failed to delete memory entry '{}' for session '{}': {error}",
                    entry_id, session_key
                ))
                .into_response();
            }
        }
    }

    let summary = form.resolved_summary();
    if entry_id.is_empty() || summary.is_empty() {
        return Redirect::to(fallback_redirect_path.as_str()).into_response();
    }

    let scope = MemoryScope {
        workspace_id: form.resolved_workspace_id(session_key.as_str()),
        channel_id: form.resolved_channel_id(),
        actor_id: form.resolved_actor_id(),
    };
    let entry = MemoryEntry {
        memory_id: entry_id.clone(),
        summary,
        tags: form.resolved_tags(),
        facts: form.resolved_facts(),
        source_event_key: form.resolved_source_event_key(entry_id.as_str()),
        recency_weight_bps: 0,
        confidence_bps: 1000,
    };
    let relation_inputs = form.resolved_relations();

    let store = gateway_memory_store(&state.config.state_dir, session_key.as_str());
    if is_edit_operation {
        match store.read_entry(entry_id.as_str(), None) {
            Ok(Some(_)) => {}
            Ok(None) => return Redirect::to(fallback_redirect_path.as_str()).into_response(),
            Err(error) => {
                return OpenResponsesApiError::internal(format!(
                    "failed to load memory entry '{}' for session '{}': {error}",
                    entry_id, session_key
                ))
                .into_response();
            }
        }
    }

    let write_result = match store.write_entry_with_metadata_and_relations(
        &scope,
        entry,
        form.resolved_memory_type(),
        form.resolved_importance(),
        relation_inputs.as_slice(),
    ) {
        Ok(result) => result,
        Err(error) => {
            return OpenResponsesApiError::internal(format!(
                "failed to upsert memory entry '{}' for session '{}': {error}",
                entry_id, session_key
            ))
            .into_response();
        }
    };

    let reason_code = if is_edit_operation {
        "ops_memory_edit_form_submitted"
    } else {
        "ops_memory_create_form_submitted"
    };
    state.record_ui_telemetry_event("memory", "entry_write", reason_code);
    record_cortex_memory_entry_write_event(
        &state.config.state_dir,
        session_key.as_str(),
        entry_id.as_str(),
        write_result.created,
    );
    let create_status = if write_result.created {
        "created"
    } else {
        "updated"
    };
    let redirect_path = build_ops_memory_redirect_path(
        form.resolved_theme(),
        form.resolved_sidebar_state(),
        session_key.as_str(),
        create_status,
        Some(entry_id.as_str()),
        "idle",
        None,
    );
    Redirect::to(redirect_path.as_str()).into_response()
}

pub(super) async fn handle_ops_dashboard_sessions_branch(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    Form(form): Form<OpsDashboardSessionBranchForm>,
) -> Response {
    let source_session_key = form.resolved_source_session_key();
    let selected_entry_id = form.resolved_entry_id();
    let redirect_theme = form.resolved_theme();
    let redirect_sidebar_state = form.resolved_sidebar_state();
    let target_session_key =
        form.resolved_target_session_key(source_session_key.as_str(), selected_entry_id);

    if target_session_key.trim().is_empty() {
        let source_redirect_path = build_ops_chat_redirect_path(
            redirect_theme,
            redirect_sidebar_state,
            source_session_key.as_str(),
        );
        return Redirect::to(source_redirect_path.as_str()).into_response();
    }

    let Some(selected_entry_id) = selected_entry_id else {
        let source_redirect_path = build_ops_chat_redirect_path(
            redirect_theme,
            redirect_sidebar_state,
            source_session_key.as_str(),
        );
        return Redirect::to(source_redirect_path.as_str()).into_response();
    };

    let source_session_path =
        gateway_session_path(&state.config.state_dir, source_session_key.as_str());
    let mut source_store = match SessionStore::load(&source_session_path) {
        Ok(store) => store,
        Err(error) => {
            return OpenResponsesApiError::internal(format!(
                "failed to load source session '{}': {error}",
                source_session_path.display()
            ))
            .into_response();
        }
    };
    source_store.set_lock_policy(
        state.config.session_lock_wait_ms,
        state.config.session_lock_stale_ms,
    );

    if !source_store.contains(selected_entry_id) {
        let source_redirect_path = build_ops_chat_redirect_path(
            redirect_theme,
            redirect_sidebar_state,
            source_session_key.as_str(),
        );
        return Redirect::to(source_redirect_path.as_str()).into_response();
    }

    let target_session_path =
        gateway_session_path(&state.config.state_dir, target_session_key.as_str());
    if let Err(error) = source_store.export_lineage(Some(selected_entry_id), &target_session_path) {
        return OpenResponsesApiError::internal(format!(
            "failed to export branch session '{}': {error}",
            target_session_path.display()
        ))
        .into_response();
    }

    state.record_ui_telemetry_event("sessions", "branch", "session_branch_created");
    let redirect_path = build_ops_chat_redirect_path(
        redirect_theme,
        redirect_sidebar_state,
        target_session_key.as_str(),
    );
    Redirect::to(redirect_path.as_str()).into_response()
}

pub(super) async fn handle_ops_dashboard_session_detail_reset(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    AxumPath(route_session_key): AxumPath<String>,
    Form(form): Form<OpsDashboardSessionResetForm>,
) -> Response {
    let route_session_key = sanitize_session_key(route_session_key.as_str());
    let session_key = form.resolved_session_key(route_session_key.as_str());
    let redirect_path = build_ops_session_detail_redirect_path(
        form.resolved_theme(),
        form.resolved_sidebar_state(),
        session_key.as_str(),
    );

    if !form.is_confirmed() {
        state.record_ui_telemetry_event("sessions", "reset", "session_reset_confirmation_missing");
        return Redirect::to(redirect_path.as_str()).into_response();
    }

    let session_path = gateway_session_path(&state.config.state_dir, session_key.as_str());
    let lock_path = session_path.with_extension("lock");
    let mut reset = false;

    if session_path.exists() {
        if let Err(error) = std::fs::remove_file(&session_path) {
            return OpenResponsesApiError::internal(format!(
                "failed to remove session '{}': {error}",
                session_path.display()
            ))
            .into_response();
        }
        reset = true;
    }
    if lock_path.exists() {
        let _ = std::fs::remove_file(&lock_path);
    }

    state.record_ui_telemetry_event("sessions", "reset", "session_reset_applied");
    record_cortex_session_reset_event(&state.config.state_dir, session_key.as_str(), reset);
    Redirect::to(redirect_path.as_str()).into_response()
}
