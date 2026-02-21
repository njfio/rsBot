//! Leptos SSR shell foundations for Tau Ops Dashboard.

use leptos::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Public enum `TauOpsDashboardAuthMode` in `tau-dashboard-ui`.
pub enum TauOpsDashboardAuthMode {
    None,
    Token,
    PasswordSession,
}

impl TauOpsDashboardAuthMode {
    /// Public `fn` `as_str` in `tau-dashboard-ui`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Token => "token",
            Self::PasswordSession => "password-session",
        }
    }

    /// Public `fn` `requires_authentication` in `tau-dashboard-ui`.
    pub fn requires_authentication(self) -> bool {
        !matches!(self, Self::None)
    }

    fn auth_input_label(self) -> &'static str {
        match self {
            Self::None => "Authentication disabled",
            Self::Token => "Bearer token",
            Self::PasswordSession => "Gateway password",
        }
    }

    fn auth_input_placeholder(self) -> &'static str {
        match self {
            Self::None => "No authentication required in localhost-dev mode",
            Self::Token => "Paste bearer token",
            Self::PasswordSession => "Enter gateway password",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Public enum `TauOpsDashboardRoute` in `tau-dashboard-ui`.
pub enum TauOpsDashboardRoute {
    Ops,
    Agents,
    AgentDetail,
    Chat,
    Sessions,
    Memory,
    MemoryGraph,
    ToolsJobs,
    Channels,
    Config,
    Training,
    Safety,
    Diagnostics,
    Deploy,
    Login,
}

impl TauOpsDashboardRoute {
    /// Public `fn` `as_str` in `tau-dashboard-ui`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ops => "ops",
            Self::Agents => "agents",
            Self::AgentDetail => "agent-detail",
            Self::Chat => "chat",
            Self::Sessions => "sessions",
            Self::Memory => "memory",
            Self::MemoryGraph => "memory-graph",
            Self::ToolsJobs => "tools-jobs",
            Self::Channels => "channels",
            Self::Config => "config",
            Self::Training => "training",
            Self::Safety => "safety",
            Self::Diagnostics => "diagnostics",
            Self::Deploy => "deploy",
            Self::Login => "login",
        }
    }

    fn breadcrumb_token(self) -> &'static str {
        match self {
            Self::Ops => "command-center",
            Self::Agents => "agent-fleet",
            Self::AgentDetail => "agent-detail",
            Self::Chat => "chat",
            Self::Sessions => "sessions",
            Self::Memory => "memory",
            Self::MemoryGraph => "memory-graph",
            Self::ToolsJobs => "tools-jobs",
            Self::Channels => "channels",
            Self::Config => "config",
            Self::Training => "training",
            Self::Safety => "safety",
            Self::Diagnostics => "diagnostics",
            Self::Deploy => "deploy",
            Self::Login => "login",
        }
    }

    fn breadcrumb_label(self) -> &'static str {
        match self {
            Self::Ops => "Command Center",
            Self::Agents => "Agent Fleet",
            Self::AgentDetail => "Agent Detail",
            Self::Chat => "Conversation / Chat",
            Self::Sessions => "Sessions Explorer",
            Self::Memory => "Memory Explorer",
            Self::MemoryGraph => "Memory Graph",
            Self::ToolsJobs => "Tools & Jobs",
            Self::Channels => "Multi-Channel",
            Self::Config => "Configuration",
            Self::Training => "Training & RL",
            Self::Safety => "Safety & Security",
            Self::Diagnostics => "Diagnostics & Audit",
            Self::Deploy => "Deploy Agent",
            Self::Login => "Login",
        }
    }

    fn shell_path(self) -> &'static str {
        match self {
            Self::Ops => "/ops",
            Self::Agents => "/ops/agents",
            Self::AgentDetail => "/ops/agents/default",
            Self::Chat => "/ops/chat",
            Self::Sessions => "/ops/sessions",
            Self::Memory => "/ops/memory",
            Self::MemoryGraph => "/ops/memory-graph",
            Self::ToolsJobs => "/ops/tools-jobs",
            Self::Channels => "/ops/channels",
            Self::Config => "/ops/config",
            Self::Training => "/ops/training",
            Self::Safety => "/ops/safety",
            Self::Diagnostics => "/ops/diagnostics",
            Self::Deploy => "/ops/deploy",
            Self::Login => "/ops/login",
        }
    }

    fn from_shell_path(route: &str) -> Option<Self> {
        match route {
            "/ops" => Some(Self::Ops),
            "/ops/agents" => Some(Self::Agents),
            "/ops/agents/default" => Some(Self::AgentDetail),
            "/ops/chat" => Some(Self::Chat),
            "/ops/sessions" => Some(Self::Sessions),
            "/ops/memory" => Some(Self::Memory),
            "/ops/memory-graph" => Some(Self::MemoryGraph),
            "/ops/tools-jobs" => Some(Self::ToolsJobs),
            "/ops/channels" => Some(Self::Channels),
            "/ops/config" => Some(Self::Config),
            "/ops/training" => Some(Self::Training),
            "/ops/safety" => Some(Self::Safety),
            "/ops/diagnostics" => Some(Self::Diagnostics),
            "/ops/deploy" => Some(Self::Deploy),
            "/ops/login" => Some(Self::Login),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Public enum `TauOpsDashboardTheme` in `tau-dashboard-ui`.
pub enum TauOpsDashboardTheme {
    Dark,
    Light,
}

impl TauOpsDashboardTheme {
    /// Public `fn` `as_str` in `tau-dashboard-ui`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Public enum `TauOpsDashboardSidebarState` in `tau-dashboard-ui`.
pub enum TauOpsDashboardSidebarState {
    Expanded,
    Collapsed,
}

impl TauOpsDashboardSidebarState {
    /// Public `fn` `as_str` in `tau-dashboard-ui`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Expanded => "expanded",
            Self::Collapsed => "collapsed",
        }
    }

    fn toggled(self) -> Self {
        match self {
            Self::Expanded => Self::Collapsed,
            Self::Collapsed => Self::Expanded,
        }
    }

    fn aria_expanded(self) -> &'static str {
        match self {
            Self::Expanded => "true",
            Self::Collapsed => "false",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `TauOpsDashboardAlertFeedRow` in `tau-dashboard-ui`.
pub struct TauOpsDashboardAlertFeedRow {
    pub code: String,
    pub severity: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `TauOpsDashboardConnectorHealthRow` in `tau-dashboard-ui`.
pub struct TauOpsDashboardConnectorHealthRow {
    pub channel: String,
    pub mode: String,
    pub liveness: String,
    pub events_ingested: u64,
    pub provider_failures: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `TauOpsDashboardChatMessageRow` in `tau-dashboard-ui`.
pub struct TauOpsDashboardChatMessageRow {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `TauOpsDashboardMemorySearchRow` in `tau-dashboard-ui`.
pub struct TauOpsDashboardMemorySearchRow {
    pub memory_id: String,
    pub summary: String,
    pub memory_type: String,
    pub score: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `TauOpsDashboardMemoryRelationRow` in `tau-dashboard-ui`.
pub struct TauOpsDashboardMemoryRelationRow {
    pub target_id: String,
    pub relation_type: String,
    pub effective_weight: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `TauOpsDashboardMemoryGraphNodeRow` in `tau-dashboard-ui`.
pub struct TauOpsDashboardMemoryGraphNodeRow {
    pub memory_id: String,
    pub memory_type: String,
    pub importance: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `TauOpsDashboardMemoryGraphEdgeRow` in `tau-dashboard-ui`.
pub struct TauOpsDashboardMemoryGraphEdgeRow {
    pub source_memory_id: String,
    pub target_memory_id: String,
    pub relation_type: String,
    pub effective_weight: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `TauOpsDashboardChatSessionOptionRow` in `tau-dashboard-ui`.
pub struct TauOpsDashboardChatSessionOptionRow {
    pub session_key: String,
    pub selected: bool,
    pub entry_count: usize,
    pub usage_total_tokens: u64,
    pub validation_is_valid: bool,
    pub updated_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `TauOpsDashboardSessionTimelineRow` in `tau-dashboard-ui`.
pub struct TauOpsDashboardSessionTimelineRow {
    pub entry_id: u64,
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `TauOpsDashboardSessionGraphNodeRow` in `tau-dashboard-ui`.
pub struct TauOpsDashboardSessionGraphNodeRow {
    pub entry_id: u64,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `TauOpsDashboardSessionGraphEdgeRow` in `tau-dashboard-ui`.
pub struct TauOpsDashboardSessionGraphEdgeRow {
    pub source_entry_id: u64,
    pub target_entry_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `TauOpsDashboardChatSnapshot` in `tau-dashboard-ui`.
pub struct TauOpsDashboardChatSnapshot {
    pub active_session_key: String,
    pub new_session_form_action: String,
    pub new_session_form_method: String,
    pub send_form_action: String,
    pub send_form_method: String,
    pub session_options: Vec<TauOpsDashboardChatSessionOptionRow>,
    pub message_rows: Vec<TauOpsDashboardChatMessageRow>,
    pub session_detail_visible: bool,
    pub session_detail_route: String,
    pub session_detail_validation_entries: usize,
    pub session_detail_validation_duplicates: usize,
    pub session_detail_validation_invalid_parent: usize,
    pub session_detail_validation_cycles: usize,
    pub session_detail_validation_is_valid: bool,
    pub session_detail_usage_input_tokens: u64,
    pub session_detail_usage_output_tokens: u64,
    pub session_detail_usage_total_tokens: u64,
    pub session_detail_usage_estimated_cost_usd: String,
    pub session_detail_timeline_rows: Vec<TauOpsDashboardSessionTimelineRow>,
    pub session_graph_node_rows: Vec<TauOpsDashboardSessionGraphNodeRow>,
    pub session_graph_edge_rows: Vec<TauOpsDashboardSessionGraphEdgeRow>,
    pub memory_search_form_action: String,
    pub memory_search_form_method: String,
    pub memory_search_query: String,
    pub memory_search_workspace_id: String,
    pub memory_search_channel_id: String,
    pub memory_search_actor_id: String,
    pub memory_search_memory_type: String,
    pub memory_search_rows: Vec<TauOpsDashboardMemorySearchRow>,
    pub memory_create_form_action: String,
    pub memory_create_form_method: String,
    pub memory_create_status: String,
    pub memory_create_created_entry_id: String,
    pub memory_create_entry_id: String,
    pub memory_create_summary: String,
    pub memory_create_tags: String,
    pub memory_create_facts: String,
    pub memory_create_source_event_key: String,
    pub memory_create_workspace_id: String,
    pub memory_create_channel_id: String,
    pub memory_create_actor_id: String,
    pub memory_create_memory_type: String,
    pub memory_create_importance: String,
    pub memory_create_relation_target_id: String,
    pub memory_create_relation_type: String,
    pub memory_create_relation_weight: String,
    pub memory_delete_status: String,
    pub memory_delete_deleted_entry_id: String,
    pub memory_detail_visible: bool,
    pub memory_detail_selected_entry_id: String,
    pub memory_detail_summary: String,
    pub memory_detail_memory_type: String,
    pub memory_detail_embedding_source: String,
    pub memory_detail_embedding_model: String,
    pub memory_detail_embedding_reason_code: String,
    pub memory_detail_embedding_dimensions: usize,
    pub memory_detail_relation_rows: Vec<TauOpsDashboardMemoryRelationRow>,
    pub memory_graph_node_rows: Vec<TauOpsDashboardMemoryGraphNodeRow>,
    pub memory_graph_edge_rows: Vec<TauOpsDashboardMemoryGraphEdgeRow>,
}

impl Default for TauOpsDashboardChatSnapshot {
    fn default() -> Self {
        Self {
            active_session_key: "default".to_string(),
            new_session_form_action: "/ops/chat/new".to_string(),
            new_session_form_method: "post".to_string(),
            send_form_action: "/ops/chat/send".to_string(),
            send_form_method: "post".to_string(),
            session_options: vec![TauOpsDashboardChatSessionOptionRow {
                session_key: "default".to_string(),
                selected: true,
                entry_count: 0,
                usage_total_tokens: 0,
                validation_is_valid: true,
                updated_unix_ms: 0,
            }],
            message_rows: vec![],
            session_detail_visible: false,
            session_detail_route: "/ops/sessions/default".to_string(),
            session_detail_validation_entries: 0,
            session_detail_validation_duplicates: 0,
            session_detail_validation_invalid_parent: 0,
            session_detail_validation_cycles: 0,
            session_detail_validation_is_valid: true,
            session_detail_usage_input_tokens: 0,
            session_detail_usage_output_tokens: 0,
            session_detail_usage_total_tokens: 0,
            session_detail_usage_estimated_cost_usd: "0.000000".to_string(),
            session_detail_timeline_rows: vec![],
            session_graph_node_rows: vec![],
            session_graph_edge_rows: vec![],
            memory_search_form_action: "/ops/memory".to_string(),
            memory_search_form_method: "get".to_string(),
            memory_search_query: String::new(),
            memory_search_workspace_id: String::new(),
            memory_search_channel_id: String::new(),
            memory_search_actor_id: String::new(),
            memory_search_memory_type: String::new(),
            memory_search_rows: vec![],
            memory_create_form_action: "/ops/memory".to_string(),
            memory_create_form_method: "post".to_string(),
            memory_create_status: "idle".to_string(),
            memory_create_created_entry_id: String::new(),
            memory_create_entry_id: String::new(),
            memory_create_summary: String::new(),
            memory_create_tags: String::new(),
            memory_create_facts: String::new(),
            memory_create_source_event_key: String::new(),
            memory_create_workspace_id: String::new(),
            memory_create_channel_id: String::new(),
            memory_create_actor_id: String::new(),
            memory_create_memory_type: String::new(),
            memory_create_importance: String::new(),
            memory_create_relation_target_id: String::new(),
            memory_create_relation_type: String::new(),
            memory_create_relation_weight: String::new(),
            memory_delete_status: "idle".to_string(),
            memory_delete_deleted_entry_id: String::new(),
            memory_detail_visible: false,
            memory_detail_selected_entry_id: String::new(),
            memory_detail_summary: String::new(),
            memory_detail_memory_type: String::new(),
            memory_detail_embedding_source: String::new(),
            memory_detail_embedding_model: String::new(),
            memory_detail_embedding_reason_code: String::new(),
            memory_detail_embedding_dimensions: 0,
            memory_detail_relation_rows: vec![],
            memory_graph_node_rows: vec![],
            memory_graph_edge_rows: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `TauOpsDashboardCommandCenterSnapshot` in `tau-dashboard-ui`.
pub struct TauOpsDashboardCommandCenterSnapshot {
    pub health_state: String,
    pub health_reason: String,
    pub rollout_gate: String,
    pub control_mode: String,
    pub control_paused: bool,
    pub action_pause_enabled: bool,
    pub action_resume_enabled: bool,
    pub action_refresh_enabled: bool,
    pub last_action_request_id: String,
    pub last_action_name: String,
    pub last_action_actor: String,
    pub last_action_timestamp_unix_ms: u64,
    pub timeline_range: String,
    pub timeline_point_count: usize,
    pub timeline_last_timestamp_unix_ms: u64,
    pub queue_depth: usize,
    pub failure_streak: usize,
    pub processed_case_count: usize,
    pub alert_count: usize,
    pub widget_count: usize,
    pub timeline_cycle_count: usize,
    pub timeline_invalid_cycle_count: usize,
    pub primary_alert_code: String,
    pub primary_alert_severity: String,
    pub primary_alert_message: String,
    pub alert_feed_rows: Vec<TauOpsDashboardAlertFeedRow>,
    pub connector_health_rows: Vec<TauOpsDashboardConnectorHealthRow>,
}

impl Default for TauOpsDashboardCommandCenterSnapshot {
    fn default() -> Self {
        Self {
            health_state: "unknown".to_string(),
            health_reason: "dashboard snapshot unavailable".to_string(),
            rollout_gate: "hold".to_string(),
            control_mode: "running".to_string(),
            control_paused: false,
            action_pause_enabled: true,
            action_resume_enabled: false,
            action_refresh_enabled: true,
            last_action_request_id: "none".to_string(),
            last_action_name: "none".to_string(),
            last_action_actor: "none".to_string(),
            last_action_timestamp_unix_ms: 0,
            timeline_range: "1h".to_string(),
            timeline_point_count: 0,
            timeline_last_timestamp_unix_ms: 0,
            queue_depth: 0,
            failure_streak: 0,
            processed_case_count: 0,
            alert_count: 0,
            widget_count: 0,
            timeline_cycle_count: 0,
            timeline_invalid_cycle_count: 0,
            primary_alert_code: "none".to_string(),
            primary_alert_severity: "info".to_string(),
            primary_alert_message: "No alerts loaded".to_string(),
            alert_feed_rows: vec![TauOpsDashboardAlertFeedRow {
                code: "none".to_string(),
                severity: "info".to_string(),
                message: "No alerts loaded".to_string(),
            }],
            connector_health_rows: vec![TauOpsDashboardConnectorHealthRow {
                channel: "none".to_string(),
                mode: "unknown".to_string(),
                liveness: "unknown".to_string(),
                events_ingested: 0,
                provider_failures: 0,
            }],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `TauOpsDashboardShellContext` in `tau-dashboard-ui`.
pub struct TauOpsDashboardShellContext {
    pub auth_mode: TauOpsDashboardAuthMode,
    pub active_route: TauOpsDashboardRoute,
    pub theme: TauOpsDashboardTheme,
    pub sidebar_state: TauOpsDashboardSidebarState,
    pub command_center: TauOpsDashboardCommandCenterSnapshot,
    pub chat: TauOpsDashboardChatSnapshot,
}

impl Default for TauOpsDashboardShellContext {
    fn default() -> Self {
        Self {
            auth_mode: TauOpsDashboardAuthMode::Token,
            active_route: TauOpsDashboardRoute::Ops,
            theme: TauOpsDashboardTheme::Dark,
            sidebar_state: TauOpsDashboardSidebarState::Expanded,
            command_center: TauOpsDashboardCommandCenterSnapshot::default(),
            chat: TauOpsDashboardChatSnapshot::default(),
        }
    }
}

fn contains_markdown_contract_syntax(content: &str) -> bool {
    content.contains("```")
        || content.starts_with('#')
        || content.contains("\n#")
        || content.starts_with("- ")
        || content.contains("\n- ")
        || content.contains("](")
        || (content.contains('|') && content.contains("\n|---"))
}

fn extract_first_fenced_code_block(content: &str) -> Option<(String, String)> {
    let fence_start = content.find("```")?;
    let after_open_fence = &content[fence_start + 3..];
    let fence_end = after_open_fence.find("```")?;
    let fenced_block = &after_open_fence[..fence_end];
    let (language, code) = if let Some((language, code)) = fenced_block.split_once('\n') {
        (language.trim(), code.trim())
    } else {
        ("plain", fenced_block.trim())
    };
    if code.is_empty() {
        return None;
    }
    let language = if language.is_empty() {
        "plain"
    } else {
        language
    };
    Some((language.to_string(), code.to_string()))
}

fn extract_assistant_stream_tokens(content: &str) -> Vec<String> {
    content
        .split_whitespace()
        .map(ToString::to_string)
        .collect()
}

/// Public `fn` `render_tau_ops_dashboard_shell` in `tau-dashboard-ui`.
pub fn render_tau_ops_dashboard_shell() -> String {
    render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext::default())
}

/// Public `fn` `render_tau_ops_dashboard_shell_for_route` in `tau-dashboard-ui`.
pub fn render_tau_ops_dashboard_shell_for_route(route: &str) -> String {
    let context = TauOpsDashboardShellContext {
        active_route: TauOpsDashboardRoute::from_shell_path(route)
            .unwrap_or(TauOpsDashboardRoute::Ops),
        ..TauOpsDashboardShellContext::default()
    };
    render_tau_ops_dashboard_shell_with_context(context)
}

/// Public `fn` `render_tau_ops_dashboard_shell_with_context` in `tau-dashboard-ui`.
pub fn render_tau_ops_dashboard_shell_with_context(context: TauOpsDashboardShellContext) -> String {
    let auth_mode = context.auth_mode;
    let login_required = auth_mode.requires_authentication();
    let auth_mode_attr = auth_mode.as_str();
    let active_route_attr = context.active_route.as_str();
    let active_shell_path = context.active_route.shell_path();
    let theme_attr = context.theme.as_str();
    let sidebar_state_attr = context.sidebar_state.as_str();
    let breadcrumb_current = context.active_route.breadcrumb_token();
    let breadcrumb_label = context.active_route.breadcrumb_label();
    let sidebar_toggle_target_state = context.sidebar_state.toggled().as_str();
    let sidebar_toggle_href =
        format!("{active_shell_path}?theme={theme_attr}&sidebar={sidebar_toggle_target_state}");
    let dark_theme_href = format!("{active_shell_path}?theme=dark&sidebar={sidebar_state_attr}");
    let light_theme_href = format!("{active_shell_path}?theme=light&sidebar={sidebar_state_attr}");
    let dark_theme_pressed = if matches!(context.theme, TauOpsDashboardTheme::Dark) {
        "true"
    } else {
        "false"
    };
    let light_theme_pressed = if matches!(context.theme, TauOpsDashboardTheme::Light) {
        "true"
    } else {
        "false"
    };
    let login_hidden = if matches!(context.active_route, TauOpsDashboardRoute::Login) {
        "false"
    } else {
        "true"
    };
    let protected_hidden = if matches!(context.active_route, TauOpsDashboardRoute::Login) {
        "true"
    } else {
        "false"
    };
    let chat_panel_hidden = if matches!(context.active_route, TauOpsDashboardRoute::Chat) {
        "false"
    } else {
        "true"
    };
    let chat_panel_visible = if matches!(context.active_route, TauOpsDashboardRoute::Chat) {
        "true"
    } else {
        "false"
    };
    let sessions_panel_hidden = if matches!(context.active_route, TauOpsDashboardRoute::Sessions) {
        "false"
    } else {
        "true"
    };
    let sessions_panel_visible = if matches!(context.active_route, TauOpsDashboardRoute::Sessions) {
        "true"
    } else {
        "false"
    };
    let memory_panel_hidden = if matches!(context.active_route, TauOpsDashboardRoute::Memory) {
        "false"
    } else {
        "true"
    };
    let memory_panel_visible = if matches!(context.active_route, TauOpsDashboardRoute::Memory) {
        "true"
    } else {
        "false"
    };
    let memory_graph_panel_hidden =
        if matches!(context.active_route, TauOpsDashboardRoute::MemoryGraph) {
            "false"
        } else {
            "true"
        };
    let memory_graph_panel_visible =
        if matches!(context.active_route, TauOpsDashboardRoute::MemoryGraph) {
            "true"
        } else {
            "false"
        };
    let command_center_panel_hidden = if matches!(context.active_route, TauOpsDashboardRoute::Ops) {
        "false"
    } else {
        "true"
    };
    let deploy_panel_hidden = if matches!(context.active_route, TauOpsDashboardRoute::Deploy) {
        "false"
    } else {
        "true"
    };
    let deploy_panel_visible = if matches!(context.active_route, TauOpsDashboardRoute::Deploy) {
        "true"
    } else {
        "false"
    };
    let chat_message_rows = if context.chat.message_rows.is_empty() {
        vec![TauOpsDashboardChatMessageRow {
            role: "system".to_string(),
            content: "No chat messages yet.".to_string(),
        }]
    } else {
        context.chat.message_rows.clone()
    };
    let sessions_row_options = context.chat.session_options.clone();
    let sessions_row_count_value = sessions_row_options.len().to_string();
    let chat_session_key = context.chat.active_session_key.clone();
    let mut chat_session_options = sessions_row_options.clone();
    let mut active_session_marked = false;
    for option in &mut chat_session_options {
        if option.session_key == chat_session_key {
            option.selected = true;
            active_session_marked = true;
        }
    }
    if !active_session_marked {
        chat_session_options.push(TauOpsDashboardChatSessionOptionRow {
            session_key: chat_session_key.clone(),
            selected: true,
            entry_count: 0,
            usage_total_tokens: 0,
            validation_is_valid: true,
            updated_unix_ms: 0,
        });
    }
    let sessions_rows_view = if sessions_row_options.is_empty() {
        leptos::either::Either::Left(view! {
            <li id="tau-ops-sessions-empty-state" data-empty-state="true">
                No sessions discovered yet.
            </li>
        })
    } else {
        leptos::either::Either::Right(
            sessions_row_options
                .iter()
                .enumerate()
                .map(|(index, session_option)| {
                    let row_id = format!("tau-ops-sessions-row-{index}");
                    let selected_attr = if session_option.selected {
                        "true"
                    } else {
                        "false"
                    };
                    let entry_count_attr = session_option.entry_count.to_string();
                    let total_tokens_attr = session_option.usage_total_tokens.to_string();
                    let is_valid_attr = if session_option.validation_is_valid {
                        "true"
                    } else {
                        "false"
                    };
                    let updated_unix_ms_attr = session_option.updated_unix_ms.to_string();
                    let open_chat_href = format!(
                        "/ops/chat?theme={theme_attr}&sidebar={sidebar_state_attr}&session={}",
                        session_option.session_key
                    );
                    view! {
                        <li
                            id=row_id
                            data-session-key=session_option.session_key.clone()
                            data-selected=selected_attr
                            data-entry-count=entry_count_attr
                            data-total-tokens=total_tokens_attr
                            data-is-valid=is_valid_attr
                            data-updated-unix-ms=updated_unix_ms_attr
                        >
                            <a
                                data-open-chat-session=session_option.session_key.clone()
                                href=open_chat_href
                            >
                                {session_option.session_key.clone()}
                            </a>
                        </li>
                    }
                })
                .collect_view(),
        )
    };
    let memory_search_form_action = context.chat.memory_search_form_action.clone();
    let memory_search_form_method = context.chat.memory_search_form_method.clone();
    let memory_search_query = context.chat.memory_search_query.clone();
    let memory_search_workspace_id = context.chat.memory_search_workspace_id.clone();
    let memory_search_channel_id = context.chat.memory_search_channel_id.clone();
    let memory_search_actor_id = context.chat.memory_search_actor_id.clone();
    let memory_search_memory_type = context.chat.memory_search_memory_type.clone();
    let memory_search_rows = context.chat.memory_search_rows.clone();
    let memory_result_count_value = memory_search_rows.len().to_string();
    let memory_query_panel_attr = memory_search_query.clone();
    let memory_query_input_value = memory_search_query.clone();
    let memory_workspace_id_panel_attr = memory_search_workspace_id.clone();
    let memory_channel_id_panel_attr = memory_search_channel_id.clone();
    let memory_actor_id_panel_attr = memory_search_actor_id.clone();
    let memory_workspace_id_input_value = memory_search_workspace_id.clone();
    let memory_channel_id_input_value = memory_search_channel_id.clone();
    let memory_actor_id_input_value = memory_search_actor_id.clone();
    let memory_type_panel_attr = memory_search_memory_type.clone();
    let memory_type_input_value = memory_search_memory_type.clone();
    let memory_result_count_panel_attr = memory_result_count_value.clone();
    let memory_result_count_list_attr = memory_result_count_value.clone();
    let memory_create_form_action = context.chat.memory_create_form_action.clone();
    let memory_create_form_method = context.chat.memory_create_form_method.clone();
    let memory_create_status = context.chat.memory_create_status.clone();
    let memory_create_created_entry_id = context.chat.memory_create_created_entry_id.clone();
    let memory_create_status_panel_attr = memory_create_status.clone();
    let memory_create_created_entry_id_panel_attr = memory_create_created_entry_id.clone();
    let memory_create_status_marker_attr = memory_create_status.clone();
    let memory_create_created_entry_id_marker_attr = memory_create_created_entry_id.clone();
    let memory_create_entry_id = context.chat.memory_create_entry_id.clone();
    let memory_create_summary = context.chat.memory_create_summary.clone();
    let memory_create_tags = context.chat.memory_create_tags.clone();
    let memory_create_facts = context.chat.memory_create_facts.clone();
    let memory_create_source_event_key = context.chat.memory_create_source_event_key.clone();
    let memory_create_workspace_id = context.chat.memory_create_workspace_id.clone();
    let memory_create_channel_id = context.chat.memory_create_channel_id.clone();
    let memory_create_actor_id = context.chat.memory_create_actor_id.clone();
    let memory_create_memory_type = context.chat.memory_create_memory_type.clone();
    let memory_create_importance = context.chat.memory_create_importance.clone();
    let memory_create_relation_target_id = context.chat.memory_create_relation_target_id.clone();
    let memory_create_relation_type = context.chat.memory_create_relation_type.clone();
    let memory_create_relation_weight = context.chat.memory_create_relation_weight.clone();
    let memory_create_status_message = match memory_create_status.as_str() {
        "created" => "Memory entry created.".to_string(),
        "updated" => "Memory entry updated.".to_string(),
        _ => "Create a memory entry.".to_string(),
    };
    let memory_edit_form_action = memory_create_form_action.clone();
    let memory_edit_form_method = memory_create_form_method.clone();
    let memory_edit_status_panel_attr = memory_create_status.clone();
    let memory_edit_edited_memory_id_panel_attr = memory_create_created_entry_id.clone();
    let memory_edit_status_marker_attr = memory_create_status.clone();
    let memory_edit_edited_memory_id_marker_attr = memory_create_created_entry_id.clone();
    let memory_edit_entry_id = memory_create_created_entry_id.clone();
    let memory_edit_summary = memory_create_summary.clone();
    let memory_edit_tags = memory_create_tags.clone();
    let memory_edit_facts = memory_create_facts.clone();
    let memory_edit_source_event_key = memory_create_source_event_key.clone();
    let memory_edit_workspace_id = memory_create_workspace_id.clone();
    let memory_edit_channel_id = memory_create_channel_id.clone();
    let memory_edit_actor_id = memory_create_actor_id.clone();
    let memory_edit_memory_type = memory_create_memory_type.clone();
    let memory_edit_importance = memory_create_importance.clone();
    let memory_edit_relation_target_id = memory_create_relation_target_id.clone();
    let memory_edit_relation_type = memory_create_relation_type.clone();
    let memory_edit_relation_weight = memory_create_relation_weight.clone();
    let memory_edit_status_message = match memory_create_status.as_str() {
        "updated" => "Memory entry updated.".to_string(),
        _ => "Edit an existing memory entry.".to_string(),
    };
    let memory_delete_form_action = memory_edit_form_action.clone();
    let memory_delete_form_method = memory_edit_form_method.clone();
    let memory_delete_status = context.chat.memory_delete_status.clone();
    let memory_delete_deleted_entry_id = context.chat.memory_delete_deleted_entry_id.clone();
    let memory_delete_status_panel_attr = memory_delete_status.clone();
    let memory_delete_deleted_entry_id_panel_attr = memory_delete_deleted_entry_id.clone();
    let memory_delete_status_marker_attr = memory_delete_status.clone();
    let memory_delete_deleted_entry_id_marker_attr = memory_delete_deleted_entry_id.clone();
    let memory_delete_entry_id = memory_delete_deleted_entry_id.clone();
    let memory_delete_status_message = match memory_delete_status.as_str() {
        "deleted" => "Memory entry deleted.".to_string(),
        _ => "Delete a memory entry.".to_string(),
    };
    let memory_detail_visible = if context.chat.memory_detail_visible {
        "true"
    } else {
        "false"
    };
    let memory_detail_selected_entry_id = context.chat.memory_detail_selected_entry_id.clone();
    let memory_detail_memory_type = context.chat.memory_detail_memory_type.clone();
    let memory_detail_embedding_source = context.chat.memory_detail_embedding_source.clone();
    let memory_detail_embedding_model = context.chat.memory_detail_embedding_model.clone();
    let memory_detail_embedding_reason_code =
        context.chat.memory_detail_embedding_reason_code.clone();
    let memory_detail_embedding_dimensions =
        context.chat.memory_detail_embedding_dimensions.to_string();
    let memory_detail_relation_rows = context.chat.memory_detail_relation_rows.clone();
    let memory_detail_relation_count = memory_detail_relation_rows.len().to_string();
    let memory_detail_embedding_source_panel_attr = memory_detail_embedding_source.clone();
    let memory_detail_embedding_model_panel_attr = memory_detail_embedding_model.clone();
    let memory_detail_embedding_reason_code_panel_attr =
        memory_detail_embedding_reason_code.clone();
    let memory_detail_embedding_dimensions_panel_attr = memory_detail_embedding_dimensions.clone();
    let memory_detail_relation_count_panel_attr = memory_detail_relation_count.clone();
    let memory_detail_summary = if context.chat.memory_detail_summary.is_empty() {
        "No selected memory detail.".to_string()
    } else {
        context.chat.memory_detail_summary.clone()
    };
    let memory_detail_relations_view = if memory_detail_relation_rows.is_empty() {
        leptos::either::Either::Left(view! {
            <li id="tau-ops-memory-relations-empty-state" data-empty-state="true">
                No connected entries.
            </li>
        })
    } else {
        leptos::either::Either::Right(
            memory_detail_relation_rows
                .iter()
                .enumerate()
                .map(|(index, row)| {
                    let row_id = format!("tau-ops-memory-relation-row-{index}");
                    view! {
                        <li
                            id=row_id
                            data-target-id=row.target_id.clone()
                            data-relation-type=row.relation_type.clone()
                            data-relation-weight=row.effective_weight.clone()
                        >
                            {row.target_id.clone()}
                        </li>
                    }
                })
                .collect_view(),
        )
    };
    let memory_results_view = if memory_search_rows.is_empty() {
        leptos::either::Either::Left(view! {
            <li id="tau-ops-memory-empty-state" data-empty-state="true">
                No memory matches found.
            </li>
        })
    } else {
        leptos::either::Either::Right(
            memory_search_rows
                .iter()
                .enumerate()
                .map(|(index, row)| {
                    let row_id = format!("tau-ops-memory-result-row-{index}");
                    view! {
                        <li
                            id=row_id
                            data-memory-id=row.memory_id.clone()
                            data-memory-type=row.memory_type.clone()
                            data-score=row.score.clone()
                            data-detail-memory-id=row.memory_id.clone()
                        >
                            {row.summary.clone()}
                        </li>
                    }
                })
                .collect_view(),
        )
    };
    let memory_graph_node_rows = context.chat.memory_graph_node_rows.clone();
    let memory_graph_edge_rows = context.chat.memory_graph_edge_rows.clone();
    let memory_graph_node_count = memory_graph_node_rows.len().to_string();
    let memory_graph_edge_count = memory_graph_edge_rows.len().to_string();
    let memory_graph_node_count_panel_attr = memory_graph_node_count.clone();
    let memory_graph_edge_count_panel_attr = memory_graph_edge_count.clone();
    let memory_graph_nodes_view = if memory_graph_node_rows.is_empty() {
        leptos::either::Either::Left(view! {
            <li id="tau-ops-memory-graph-empty-state" data-empty-state="true">
                No memory graph nodes available.
            </li>
        })
    } else {
        leptos::either::Either::Right(
            memory_graph_node_rows
                .iter()
                .enumerate()
                .map(|(index, row)| {
                    let row_id = format!("tau-ops-memory-graph-node-{index}");
                    view! {
                        <li
                            id=row_id
                            data-memory-id=row.memory_id.clone()
                            data-memory-type=row.memory_type.clone()
                            data-importance=row.importance.clone()
                        ></li>
                    }
                })
                .collect_view(),
        )
    };
    let memory_graph_edges_view = memory_graph_edge_rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let row_id = format!("tau-ops-memory-graph-edge-{index}");
            view! {
                <li
                    id=row_id
                    data-source-memory-id=row.source_memory_id.clone()
                    data-target-memory-id=row.target_memory_id.clone()
                    data-relation-type=row.relation_type.clone()
                    data-relation-weight=row.effective_weight.clone()
                ></li>
            }
        })
        .collect_view();
    let session_detail_panel_hidden =
        if matches!(context.active_route, TauOpsDashboardRoute::Sessions)
            && context.chat.session_detail_visible
        {
            "false"
        } else {
            "true"
        };
    let session_detail_route = context.chat.session_detail_route.clone();
    let session_graph_route = session_detail_route.clone();
    let session_reset_form_action = session_detail_route.clone();
    let session_detail_validation_entries =
        context.chat.session_detail_validation_entries.to_string();
    let session_detail_validation_duplicates = context
        .chat
        .session_detail_validation_duplicates
        .to_string();
    let session_detail_validation_invalid_parent = context
        .chat
        .session_detail_validation_invalid_parent
        .to_string();
    let session_detail_validation_cycles =
        context.chat.session_detail_validation_cycles.to_string();
    let session_detail_validation_is_valid = if context.chat.session_detail_validation_is_valid {
        "true"
    } else {
        "false"
    };
    let session_detail_usage_input_tokens =
        context.chat.session_detail_usage_input_tokens.to_string();
    let session_detail_usage_output_tokens =
        context.chat.session_detail_usage_output_tokens.to_string();
    let session_detail_usage_total_tokens =
        context.chat.session_detail_usage_total_tokens.to_string();
    let session_detail_usage_estimated_cost_usd =
        context.chat.session_detail_usage_estimated_cost_usd.clone();
    let session_detail_timeline_rows = context.chat.session_detail_timeline_rows.clone();
    let session_detail_timeline_count = session_detail_timeline_rows.len().to_string();
    let session_detail_timeline_view = if session_detail_timeline_rows.is_empty() {
        leptos::either::Either::Left(view! {
            <li id="tau-ops-session-message-empty-state" data-empty-state="true">
                No session timeline entries yet.
            </li>
        })
    } else {
        leptos::either::Either::Right(
            session_detail_timeline_rows
                .iter()
                .enumerate()
                .map(|(index, row)| {
                    let row_id = format!("tau-ops-session-message-row-{index}");
                    let entry_id = row.entry_id.to_string();
                    let row_entry_id_value = entry_id.clone();
                    let row_content_attr = row.content.clone();
                    let row_content_body = row.content.clone();
                    let form_entry_id_value = entry_id.clone();
                    let hidden_entry_id_value = entry_id.clone();
                    let branch_form_id = format!("tau-ops-session-branch-form-{index}");
                    let branch_source_id =
                        format!("tau-ops-session-branch-source-session-key-{index}");
                    let branch_entry_id = format!("tau-ops-session-branch-entry-id-{index}");
                    let branch_target_id =
                        format!("tau-ops-session-branch-target-session-key-{index}");
                    let branch_theme_id = format!("tau-ops-session-branch-theme-{index}");
                    let branch_sidebar_id = format!("tau-ops-session-branch-sidebar-{index}");
                    let branch_submit_id = format!("tau-ops-session-branch-submit-{index}");
                    view! {
                        <li
                            id=row_id
                            data-entry-id=row_entry_id_value
                            data-message-role=row.role.clone()
                            data-message-content=row_content_attr
                        >
                            {row_content_body}
                            <form
                                id=branch_form_id
                                action="/ops/sessions/branch"
                                method="post"
                                data-source-session-key=chat_session_key.clone()
                                data-entry-id=form_entry_id_value
                            >
                                <input
                                    id=branch_source_id
                                    type="hidden"
                                    name="source_session_key"
                                    value=chat_session_key.clone()
                                />
                                <input
                                    id=branch_entry_id
                                    type="hidden"
                                    name="entry_id"
                                    value=hidden_entry_id_value
                                />
                                <label for=branch_target_id.clone()>Branch Session Key</label>
                                <input
                                    id=branch_target_id
                                    type="text"
                                    name="target_session_key"
                                    value=""
                                />
                                <input id=branch_theme_id type="hidden" name="theme" value=theme_attr />
                                <input
                                    id=branch_sidebar_id
                                    type="hidden"
                                    name="sidebar"
                                    value=sidebar_state_attr
                                />
                                <button
                                    id=branch_submit_id
                                    type="submit"
                                    data-confirmation-required="true"
                                >
                                    Branch Session
                                </button>
                            </form>
                        </li>
                    }
                })
                .collect_view(),
        )
    };
    let session_graph_node_rows = context.chat.session_graph_node_rows.clone();
    let session_graph_edge_rows = context.chat.session_graph_edge_rows.clone();
    let session_graph_node_count = session_graph_node_rows.len().to_string();
    let session_graph_edge_count = session_graph_edge_rows.len().to_string();
    let session_graph_view = if session_graph_node_rows.is_empty() {
        leptos::either::Either::Left(view! {
            <li id="tau-ops-session-graph-empty-state" data-empty-state="true">
                No session graph nodes yet.
            </li>
        })
    } else {
        leptos::either::Either::Right(
            session_graph_node_rows
                .iter()
                .enumerate()
                .map(|(index, row)| {
                    let row_id = format!("tau-ops-session-graph-node-{index}");
                    let entry_id = row.entry_id.to_string();
                    view! {
                        <li id=row_id data-entry-id=entry_id data-message-role=row.role.clone()></li>
                    }
                })
                .collect_view(),
        )
    };
    let session_graph_edges_view = session_graph_edge_rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let row_id = format!("tau-ops-session-graph-edge-{index}");
            let source_entry_id = row.source_entry_id.to_string();
            let target_entry_id = row.target_entry_id.to_string();
            view! {
                <li
                    id=row_id
                    data-source-entry-id=source_entry_id
                    data-target-entry-id=target_entry_id
                ></li>
            }
        })
        .collect_view();
    let chat_session_option_count_value = chat_session_options.len().to_string();
    let chat_message_count_value = chat_message_rows.len().to_string();
    let chat_new_session_form_action = context.chat.new_session_form_action.clone();
    let chat_new_session_form_method = context.chat.new_session_form_method.clone();
    let chat_send_form_action = context.chat.send_form_action.clone();
    let chat_send_form_method = context.chat.send_form_method.clone();
    let health_state = context.command_center.health_state.clone();
    let health_reason = context.command_center.health_reason.clone();
    let rollout_gate = context.command_center.rollout_gate.clone();
    let control_mode = context.command_center.control_mode.clone();
    let control_paused_value = if context.command_center.control_paused {
        "true"
    } else {
        "false"
    };
    let action_pause_enabled_value = if context.command_center.action_pause_enabled {
        "true"
    } else {
        "false"
    };
    let action_resume_enabled_value = if context.command_center.action_resume_enabled {
        "true"
    } else {
        "false"
    };
    let action_refresh_enabled_value = if context.command_center.action_refresh_enabled {
        "true"
    } else {
        "false"
    };
    let last_action_request_id = context.command_center.last_action_request_id.clone();
    let last_action_name = context.command_center.last_action_name.clone();
    let last_action_actor = context.command_center.last_action_actor.clone();
    let last_action_timestamp_value = context
        .command_center
        .last_action_timestamp_unix_ms
        .to_string();
    let timeline_range = context.command_center.timeline_range.clone();
    let timeline_point_count_value = context.command_center.timeline_point_count.to_string();
    let timeline_last_timestamp_value = context
        .command_center
        .timeline_last_timestamp_unix_ms
        .to_string();
    let range_1h_selected = if timeline_range == "1h" {
        "true"
    } else {
        "false"
    };
    let range_6h_selected = if timeline_range == "6h" {
        "true"
    } else {
        "false"
    };
    let range_24h_selected = if timeline_range == "24h" {
        "true"
    } else {
        "false"
    };
    let range_1h_href =
        format!("{active_shell_path}?theme={theme_attr}&sidebar={sidebar_state_attr}&range=1h");
    let range_6h_href =
        format!("{active_shell_path}?theme={theme_attr}&sidebar={sidebar_state_attr}&range=6h");
    let range_24h_href =
        format!("{active_shell_path}?theme={theme_attr}&sidebar={sidebar_state_attr}&range=24h");
    let queue_depth_value = context.command_center.queue_depth.to_string();
    let failure_streak_value = context.command_center.failure_streak.to_string();
    let processed_cases_value = context.command_center.processed_case_count.to_string();
    let alert_count_value = context.command_center.alert_count.to_string();
    let alert_count_feed_value = alert_count_value.clone();
    let alert_feed_rows = if context.command_center.alert_feed_rows.is_empty() {
        vec![TauOpsDashboardAlertFeedRow {
            code: context.command_center.primary_alert_code.clone(),
            severity: context.command_center.primary_alert_severity.clone(),
            message: context.command_center.primary_alert_message.clone(),
        }]
    } else {
        context.command_center.alert_feed_rows.clone()
    };
    let alert_row_count_value = alert_feed_rows.len().to_string();
    let alert_row_count_section_value = alert_row_count_value.clone();
    let alert_row_count_list_value = alert_row_count_value;
    let connector_health_rows = if context.command_center.connector_health_rows.is_empty() {
        vec![TauOpsDashboardConnectorHealthRow {
            channel: "none".to_string(),
            mode: "unknown".to_string(),
            liveness: "unknown".to_string(),
            events_ingested: 0,
            provider_failures: 0,
        }]
    } else {
        context.command_center.connector_health_rows.clone()
    };
    let connector_row_count_value = connector_health_rows.len().to_string();
    let connector_row_count_table_value = connector_row_count_value.clone();
    let connector_row_count_body_value = connector_row_count_value;
    let widget_count_value = context.command_center.widget_count.to_string();
    let timeline_cycle_count_value = context.command_center.timeline_cycle_count.to_string();
    let timeline_cycle_count_table_value = timeline_cycle_count_value.clone();
    let timeline_cycle_count_summary_value = timeline_cycle_count_table_value.clone();
    let timeline_point_count_table_value = timeline_point_count_value.clone();
    let timeline_last_timestamp_table_value = timeline_last_timestamp_value.clone();
    let timeline_invalid_cycle_count_value = context
        .command_center
        .timeline_invalid_cycle_count
        .to_string();
    let timeline_invalid_cycle_count_summary_value = timeline_invalid_cycle_count_value.clone();
    let timeline_empty_row = if context.command_center.timeline_point_count == 0 {
        Some(view! {
            <tr id="tau-ops-timeline-empty-row" data-empty-state="true">
                <td colspan="4">No timeline points yet.</td>
            </tr>
        })
    } else {
        None
    };
    let primary_alert_code = context.command_center.primary_alert_code.clone();
    let primary_alert_severity = context.command_center.primary_alert_severity.clone();
    let primary_alert_message = context.command_center.primary_alert_message.clone();

    let shell = view! {
        <div
            id="tau-ops-shell"
            data-app="tau-ops-dashboard"
            data-theme=theme_attr
            data-sidebar-state=sidebar_state_attr
            data-sidebar-mobile-default="collapsed"
        >
            <header id="tau-ops-header">
                <h1>Tau Ops Dashboard</h1>
                <p>Leptos SSR foundation shell</p>
                <a
                    id="tau-ops-skip-to-main"
                    href="#tau-ops-protected-shell"
                    data-keyboard-navigation="true"
                >
                    Skip to main content
                </a>
                <div id="tau-ops-shell-controls">
                    <input
                        id="tau-ops-sidebar-toggle"
                        type="checkbox"
                        data-sidebar-state=sidebar_state_attr
                        aria-hidden="true"
                    />
                    <a
                        id="tau-ops-sidebar-hamburger"
                        data-sidebar-toggle="true"
                        data-sidebar-target-state=sidebar_toggle_target_state
                        aria-controls="tau-ops-sidebar"
                        aria-expanded=context.sidebar_state.aria_expanded()
                        href=sidebar_toggle_href
                    >
                        Toggle Navigation
                    </a>
                    <div id="tau-ops-theme-controls" role="group" aria-label="Theme controls">
                        <a
                            id="tau-ops-theme-toggle-dark"
                            data-theme-option="dark"
                            aria-pressed=dark_theme_pressed
                            href=dark_theme_href
                        >
                            Dark
                        </a>
                        <a
                            id="tau-ops-theme-toggle-light"
                            data-theme-option="light"
                            aria-pressed=light_theme_pressed
                            href=light_theme_href
                        >
                            Light
                        </a>
                    </div>
                </div>
                <nav
                    id="tau-ops-breadcrumbs"
                    aria-label="Tau Ops breadcrumbs"
                    data-breadcrumb-current=breadcrumb_current
                >
                    <ol>
                        <li id="tau-ops-breadcrumb-home">
                            <a href="/ops">Home</a>
                        </li>
                        <li id="tau-ops-breadcrumb-current">{breadcrumb_label}</li>
                    </ol>
                </nav>
            </header>
            <div id="tau-ops-layout">
                <aside id="tau-ops-sidebar">
                    <nav aria-label="Tau Ops navigation">
                        <ul>
                            <li id="tau-ops-nav-command-center"><a data-nav-item="command-center" href="/ops">Command Center</a></li>
                            <li id="tau-ops-nav-agent-fleet"><a data-nav-item="agent-fleet" href="/ops/agents">Agent Fleet</a></li>
                            <li id="tau-ops-nav-agent-detail"><a data-nav-item="agent-detail" href="/ops/agents/default">Agent Detail</a></li>
                            <li id="tau-ops-nav-chat"><a data-nav-item="chat" href="/ops/chat">Conversation / Chat</a></li>
                            <li id="tau-ops-nav-sessions"><a data-nav-item="sessions" href="/ops/sessions">Sessions Explorer</a></li>
                            <li id="tau-ops-nav-memory"><a data-nav-item="memory" href="/ops/memory">Memory Explorer</a></li>
                            <li id="tau-ops-nav-memory-graph"><a data-nav-item="memory-graph" href="/ops/memory-graph">Memory Graph</a></li>
                            <li id="tau-ops-nav-tools-jobs"><a data-nav-item="tools-jobs" href="/ops/tools-jobs">Tools & Jobs</a></li>
                            <li id="tau-ops-nav-channels"><a data-nav-item="channels" href="/ops/channels">Multi-Channel</a></li>
                            <li id="tau-ops-nav-config"><a data-nav-item="config" href="/ops/config">Configuration</a></li>
                            <li id="tau-ops-nav-training"><a data-nav-item="training" href="/ops/training">Training & RL</a></li>
                            <li id="tau-ops-nav-safety"><a data-nav-item="safety" href="/ops/safety">Safety & Security</a></li>
                            <li id="tau-ops-nav-diagnostics"><a data-nav-item="diagnostics" href="/ops/diagnostics">Diagnostics & Audit</a></li>
                            <li id="tau-ops-nav-deploy"><a data-nav-item="deploy" href="/ops/deploy">Deploy Agent</a></li>
                            <li id="tau-ops-nav-login"><a href="/ops/login">Operator Login</a></li>
                            <li id="tau-ops-nav-legacy-dashboard"><a href="/dashboard">Legacy Dashboard</a></li>
                            <li id="tau-ops-nav-webchat"><a href="/webchat">Webchat</a></li>
                        </ul>
                    </nav>
                </aside>
                <div
                    id="tau-ops-auth-shell"
                    data-auth-mode=auth_mode_attr
                    data-login-required=if login_required { "true" } else { "false" }
                    data-active-route=active_route_attr
                >
                    <section id="tau-ops-login-shell" data-route="/ops/login" aria-hidden=login_hidden>
                        <h2>Operator Authentication</h2>
                        <p>
                            Use configured gateway auth mode to continue to protected operations views.
                        </p>
                        <form id="tau-ops-login-form">
                            <label for="tau-ops-auth-input">{auth_mode.auth_input_label()}</label>
                            <input
                                id="tau-ops-auth-input"
                                type="password"
                                autocomplete="off"
                                placeholder=auth_mode.auth_input_placeholder()
                            />
                            <button id="tau-ops-login-submit" type="button">Continue</button>
                        </form>
                    </section>
                    <main id="tau-ops-protected-shell" data-route="/ops" aria-hidden=protected_hidden>
                        <section
                            id="tau-ops-accessibility-contract"
                            data-component="AccessibilityContract"
                            data-axe-contract="required"
                            data-keyboard-navigation="true"
                            data-focus-visible-contract="true"
                            data-focus-ring-token="tau-focus-ring"
                            data-reduced-motion-contract="prefers-reduced-motion"
                            data-reduced-motion-behavior="suppress-nonessential-animation"
                        >
                            <h2>Accessibility Contracts</h2>
                            <p id="tau-ops-live-announcer" aria-live="polite" aria-atomic="true">
                                Accessibility live region ready.
                            </p>
                        </section>
                        <section
                            id="tau-ops-stream-contract"
                            data-component="RealtimeStreamContract"
                            data-stream-transport="websocket"
                            data-stream-connect-on-load="true"
                            data-heartbeat-target="tau-ops-kpi-grid"
                            data-alert-feed-target="tau-ops-alert-feed-list"
                            data-chat-stream-mode="websocket"
                            data-chat-polling="disabled"
                            data-connector-health-target="tau-ops-connector-table-body"
                            data-reconnect-strategy="exponential-backoff"
                            data-reconnect-base-ms="250"
                            data-reconnect-max-ms="8000"
                        >
                            <h2>Real-Time Stream Contracts</h2>
                        </section>
                        <section
                            id="tau-ops-performance-contract"
                            data-component="PerformanceBudgetContract"
                            data-wasm-budget-gzip-kb="500"
                            data-lcp-budget-ms="1500"
                            data-layout-shift-budget="0.00"
                            data-layout-shift-mitigation="skeletons"
                            data-websocket-process-budget-ms="50"
                        >
                            <h2>Performance Budgets</h2>
                        </section>
                        <section
                            id="tau-ops-chat-panel"
                            data-route="/ops/chat"
                            aria-hidden=chat_panel_hidden
                            data-active-session-key=chat_session_key.clone()
                            data-panel-visible=chat_panel_visible
                        >
                            <h2>Conversation / Chat</h2>
                            <section
                                id="tau-ops-chat-session-selector"
                                data-active-session-key=chat_session_key.clone()
                                data-option-count=chat_session_option_count_value
                            >
                                <ul id="tau-ops-chat-session-options">
                                    {chat_session_options
                                        .iter()
                                        .enumerate()
                                        .map(|(index, session_option)| {
                                            let session_option_row_id =
                                                format!("tau-ops-chat-session-option-{index}");
                                            let selected_attr = if session_option.selected {
                                                "true"
                                            } else {
                                                "false"
                                            };
                                            let session_href = format!(
                                                "/ops/chat?theme={theme_attr}&sidebar={sidebar_state_attr}&session={}",
                                                session_option.session_key
                                            );
                                            view! {
                                                <li
                                                    id=session_option_row_id
                                                    data-session-key=session_option.session_key.clone()
                                                    data-selected=selected_attr
                                                >
                                                    <a
                                                        data-session-link=session_option.session_key.clone()
                                                        href=session_href
                                                    >
                                                        {session_option.session_key.clone()}
                                                    </a>
                                                </li>
                                            }
                                        })
                                        .collect_view()}
                                </ul>
                            </section>
                            <form
                                id="tau-ops-chat-new-session-form"
                                action=chat_new_session_form_action
                                method=chat_new_session_form_method
                                data-active-session-key=chat_session_key.clone()
                            >
                                <label for="tau-ops-chat-new-session-key">New Session</label>
                                <input
                                    id="tau-ops-chat-new-session-key"
                                    type="text"
                                    name="session_key"
                                    value=""
                                    autocomplete="off"
                                />
                                <input
                                    id="tau-ops-chat-new-theme"
                                    type="hidden"
                                    name="theme"
                                    value=theme_attr
                                />
                                <input
                                    id="tau-ops-chat-new-sidebar"
                                    type="hidden"
                                    name="sidebar"
                                    value=sidebar_state_attr
                                />
                                <button id="tau-ops-chat-new-session-button" type="submit">
                                    Create Session
                                </button>
                            </form>
                            <form
                                id="tau-ops-chat-send-form"
                                action=chat_send_form_action
                                method=chat_send_form_method
                                data-session-key=chat_session_key.clone()
                            >
                                <input
                                    id="tau-ops-chat-session-key"
                                    type="hidden"
                                    name="session_key"
                                    value=chat_session_key.clone()
                                />
                                <input id="tau-ops-chat-theme" type="hidden" name="theme" value=theme_attr />
                                <input
                                    id="tau-ops-chat-sidebar"
                                    type="hidden"
                                    name="sidebar"
                                    value=sidebar_state_attr
                                />
                                <label for="tau-ops-chat-input">Message</label>
                                <p
                                    id="tau-ops-chat-input-shortcut-hint"
                                    data-shortcut-contract="shift-enter"
                                >
                                    Shift+Enter inserts a newline in the message editor.
                                </p>
                                <textarea
                                    id="tau-ops-chat-input"
                                    name="message"
                                    placeholder="Type a message for the active session"
                                    rows="4"
                                    data-multiline-enabled="true"
                                    data-newline-shortcut="shift-enter"
                                ></textarea>
                                <button id="tau-ops-chat-send-button" type="submit">Send</button>
                            </form>
                            <ul id="tau-ops-chat-transcript" data-message-count=chat_message_count_value>
                                {chat_message_rows
                                    .iter()
                                    .enumerate()
                                    .map(|(index, message_row)| {
                                        let row_id = format!("tau-ops-chat-message-row-{index}");
                                        if message_row.role == "tool" {
                                            let tool_card_id =
                                                format!("tau-ops-chat-tool-card-{index}");
                                            view! {
                                                <li id=row_id data-message-role=message_row.role.clone()>
                                                    <article
                                                        id=tool_card_id
                                                        data-tool-card="true"
                                                        data-inline-result="true"
                                                    >
                                                        {message_row.content.clone()}
                                                    </article>
                                                </li>
                                            }
                                            .into_any()
                                        } else if message_row.role == "assistant" {
                                            let assistant_tokens =
                                                extract_assistant_stream_tokens(&message_row.content);
                                            let assistant_token_count =
                                                assistant_tokens.len().to_string();
                                            let assistant_token_stream_id =
                                                format!("tau-ops-chat-token-stream-{index}");
                                            let render_assistant_tokens = || {
                                                assistant_tokens
                                                    .iter()
                                                    .enumerate()
                                                    .map(|(token_index, token)| {
                                                        let token_id = format!(
                                                            "tau-ops-chat-token-{index}-{token_index}"
                                                        );
                                                        let token_index_attr = token_index.to_string();
                                                        view! {
                                                            <li
                                                                id=token_id
                                                                data-token-index=token_index_attr
                                                                data-token-value=token.clone()
                                                            >
                                                                {token.clone()}
                                                            </li>
                                                        }
                                                    })
                                                    .collect_view()
                                            };
                                            let markdown_contract =
                                                contains_markdown_contract_syntax(&message_row.content);
                                            let code_block_contract =
                                                extract_first_fenced_code_block(&message_row.content);
                                            if markdown_contract || code_block_contract.is_some() {
                                                let token_count_row_attr =
                                                    assistant_token_count.clone();
                                                let token_count_stream_attr =
                                                    assistant_token_count.clone();
                                                let content_view = if markdown_contract {
                                                    let markdown_card_id =
                                                        format!("tau-ops-chat-markdown-{index}");
                                                    view! {
                                                        <article
                                                            id=markdown_card_id
                                                            data-markdown-rendered="true"
                                                        >
                                                            {message_row.content.clone()}
                                                        </article>
                                                    }
                                                    .into_any()
                                                } else {
                                                    view! { {message_row.content.clone()} }.into_any()
                                                };
                                                let code_view = code_block_contract.map(
                                                    |(language, code)| {
                                                        let code_block_id =
                                                            format!("tau-ops-chat-code-block-{index}");
                                                        let code_attribute = code.clone();
                                                        view! {
                                                            <pre
                                                                id=code_block_id
                                                                data-code-block="true"
                                                                data-language=language.clone()
                                                                data-code=code_attribute
                                                            >
                                                                {code}
                                                            </pre>
                                                        }
                                                    },
                                                );
                                                view! {
                                                    <li
                                                        id=row_id
                                                        data-message-role=message_row.role.clone()
                                                        data-assistant-token-stream="true"
                                                        data-token-count=token_count_row_attr
                                                    >
                                                        {content_view}
                                                        {code_view}
                                                        <ol
                                                            id=assistant_token_stream_id.clone()
                                                            data-token-stream="assistant"
                                                            data-token-count=token_count_stream_attr
                                                        >
                                                            {render_assistant_tokens()}
                                                        </ol>
                                                    </li>
                                                }
                                                .into_any()
                                            } else {
                                                let token_count_row_attr =
                                                    assistant_token_count.clone();
                                                let token_count_stream_attr =
                                                    assistant_token_count.clone();
                                                view! {
                                                    <li
                                                        id=row_id
                                                        data-message-role=message_row.role.clone()
                                                        data-assistant-token-stream="true"
                                                        data-token-count=token_count_row_attr
                                                    >
                                                        {message_row.content.clone()}
                                                        <ol
                                                            id=assistant_token_stream_id
                                                            data-token-stream="assistant"
                                                            data-token-count=token_count_stream_attr
                                                        >
                                                            {render_assistant_tokens()}
                                                        </ol>
                                                    </li>
                                                }
                                                .into_any()
                                            }
                                        } else {
                                            view! {
                                                <li id=row_id data-message-role=message_row.role.clone()>
                                                    {message_row.content.clone()}
                                                </li>
                                            }
                                            .into_any()
                                        }
                                    })
                                    .collect_view()}
                            </ul>
                            <article
                                id="tau-ops-chat-token-counter"
                                data-session-key=chat_session_key.clone()
                                data-input-tokens=session_detail_usage_input_tokens.clone()
                                data-output-tokens=session_detail_usage_output_tokens.clone()
                                data-total-tokens=session_detail_usage_total_tokens.clone()
                            >
                                Token Counter
                            </article>
                        </section>
                        <section
                            id="tau-ops-sessions-panel"
                            data-route="/ops/sessions"
                            aria-hidden=sessions_panel_hidden
                            data-panel-visible=sessions_panel_visible
                        >
                            <h2>Sessions Explorer</h2>
                            <ul id="tau-ops-sessions-list" data-session-count=sessions_row_count_value>
                                {sessions_rows_view}
                            </ul>
                        </section>
                        <section
                            id="tau-ops-session-detail-panel"
                            data-route=session_detail_route
                            data-session-key=chat_session_key.clone()
                            aria-hidden=session_detail_panel_hidden
                        >
                            <h2>Session Detail</h2>
                            <form
                                id="tau-ops-session-reset-form"
                                action=session_reset_form_action
                                method="post"
                                data-session-key=chat_session_key.clone()
                                data-confirmation-required="true"
                            >
                                <input
                                    id="tau-ops-session-reset-session-key"
                                    type="hidden"
                                    name="session_key"
                                    value=chat_session_key.clone()
                                />
                                <input id="tau-ops-session-reset-theme" type="hidden" name="theme" value=theme_attr />
                                <input
                                    id="tau-ops-session-reset-sidebar"
                                    type="hidden"
                                    name="sidebar"
                                    value=sidebar_state_attr
                                />
                                <input
                                    id="tau-ops-session-reset-confirm"
                                    type="hidden"
                                    name="confirm_reset"
                                    value="true"
                                />
                                <button
                                    id="tau-ops-session-reset-submit"
                                    type="submit"
                                    data-confirmation-required="true"
                                >
                                    Reset Session
                                </button>
                            </form>
                            <article
                                id="tau-ops-session-validation-report"
                                data-entries=session_detail_validation_entries
                                data-duplicates=session_detail_validation_duplicates
                                data-invalid-parent=session_detail_validation_invalid_parent
                                data-cycles=session_detail_validation_cycles
                                data-is-valid=session_detail_validation_is_valid
                            >
                                Validation Summary
                            </article>
                            <article
                                id="tau-ops-session-usage-summary"
                                data-input-tokens=session_detail_usage_input_tokens
                                data-output-tokens=session_detail_usage_output_tokens
                                data-total-tokens=session_detail_usage_total_tokens
                                data-estimated-cost-usd=session_detail_usage_estimated_cost_usd
                            >
                                Usage Summary
                            </article>
                            <ul
                                id="tau-ops-session-message-timeline"
                                data-entry-count=session_detail_timeline_count
                            >
                                {session_detail_timeline_view}
                            </ul>
                        </section>
                        <section
                            id="tau-ops-session-graph-panel"
                            data-route=session_graph_route
                            data-session-key=chat_session_key.clone()
                            aria-hidden=session_detail_panel_hidden
                        >
                            <h2>Session Graph</h2>
                            <ul id="tau-ops-session-graph-nodes" data-node-count=session_graph_node_count>
                                {session_graph_view}
                            </ul>
                            <ul id="tau-ops-session-graph-edges" data-edge-count=session_graph_edge_count>
                                {session_graph_edges_view}
                            </ul>
                        </section>
                        <section
                            id="tau-ops-memory-panel"
                            data-route="/ops/memory"
                            aria-hidden=memory_panel_hidden
                            data-panel-visible=memory_panel_visible
                            data-query=memory_query_panel_attr
                            data-result-count=memory_result_count_panel_attr
                            data-workspace-id=memory_workspace_id_panel_attr
                            data-channel-id=memory_channel_id_panel_attr
                            data-actor-id=memory_actor_id_panel_attr
                            data-memory-type=memory_type_panel_attr
                            data-create-status=memory_create_status_panel_attr
                            data-created-memory-id=memory_create_created_entry_id_panel_attr
                            data-edit-status=memory_edit_status_panel_attr
                            data-edited-memory-id=memory_edit_edited_memory_id_panel_attr
                            data-delete-status=memory_delete_status_panel_attr
                            data-deleted-memory-id=memory_delete_deleted_entry_id_panel_attr
                        >
                            <h2>Memory Explorer</h2>
                            <form
                                id="tau-ops-memory-search-form"
                                action=memory_search_form_action
                                method=memory_search_form_method
                            >
                                <input id="tau-ops-memory-theme" type="hidden" name="theme" value=theme_attr />
                                <input
                                    id="tau-ops-memory-sidebar"
                                    type="hidden"
                                    name="sidebar"
                                    value=sidebar_state_attr
                                />
                                <input
                                    id="tau-ops-memory-session"
                                    type="hidden"
                                    name="session"
                                    value=chat_session_key.clone()
                                />
                                <label for="tau-ops-memory-query">Search Memory</label>
                                <input
                                    id="tau-ops-memory-query"
                                    type="search"
                                    name="query"
                                    value=memory_query_input_value
                                />
                                <label for="tau-ops-memory-workspace-filter">Workspace</label>
                                <input
                                    id="tau-ops-memory-workspace-filter"
                                    type="text"
                                    name="workspace_id"
                                    value=memory_workspace_id_input_value
                                />
                                <label for="tau-ops-memory-channel-filter">Channel</label>
                                <input
                                    id="tau-ops-memory-channel-filter"
                                    type="text"
                                    name="channel_id"
                                    value=memory_channel_id_input_value
                                />
                                <label for="tau-ops-memory-actor-filter">Actor</label>
                                <input
                                    id="tau-ops-memory-actor-filter"
                                    type="text"
                                    name="actor_id"
                                    value=memory_actor_id_input_value
                                />
                                <label for="tau-ops-memory-type-filter">Memory Type</label>
                                <input
                                    id="tau-ops-memory-type-filter"
                                    type="text"
                                    name="memory_type"
                                    value=memory_type_input_value
                                />
                                <button id="tau-ops-memory-search-button" type="submit">
                                    Search
                                </button>
                            </form>
                            <p
                                id="tau-ops-memory-create-status"
                                data-create-status=memory_create_status_marker_attr
                                data-created-memory-id=memory_create_created_entry_id_marker_attr
                            >
                                {memory_create_status_message}
                            </p>
                            <form
                                id="tau-ops-memory-create-form"
                                action=memory_create_form_action
                                method=memory_create_form_method
                            >
                                <input id="tau-ops-memory-create-theme" type="hidden" name="theme" value=theme_attr />
                                <input
                                    id="tau-ops-memory-create-sidebar"
                                    type="hidden"
                                    name="sidebar"
                                    value=sidebar_state_attr
                                />
                                <input
                                    id="tau-ops-memory-create-session"
                                    type="hidden"
                                    name="session"
                                    value=chat_session_key.clone()
                                />
                                <input
                                    id="tau-ops-memory-create-operation"
                                    type="hidden"
                                    name="operation"
                                    value="create"
                                />
                                <label for="tau-ops-memory-create-entry-id">Entry ID</label>
                                <input
                                    id="tau-ops-memory-create-entry-id"
                                    type="text"
                                    name="entry_id"
                                    value=memory_create_entry_id
                                />
                                <label for="tau-ops-memory-create-summary">Summary</label>
                                <input
                                    id="tau-ops-memory-create-summary"
                                    type="text"
                                    name="summary"
                                    value=memory_create_summary
                                />
                                <label for="tau-ops-memory-create-tags">Tags</label>
                                <input
                                    id="tau-ops-memory-create-tags"
                                    type="text"
                                    name="tags"
                                    value=memory_create_tags
                                />
                                <label for="tau-ops-memory-create-facts">Facts</label>
                                <input
                                    id="tau-ops-memory-create-facts"
                                    type="text"
                                    name="facts"
                                    value=memory_create_facts
                                />
                                <label for="tau-ops-memory-create-source-event-key">Source Event Key</label>
                                <input
                                    id="tau-ops-memory-create-source-event-key"
                                    type="text"
                                    name="source_event_key"
                                    value=memory_create_source_event_key
                                />
                                <label for="tau-ops-memory-create-workspace-id">Workspace</label>
                                <input
                                    id="tau-ops-memory-create-workspace-id"
                                    type="text"
                                    name="workspace_id"
                                    value=memory_create_workspace_id
                                />
                                <label for="tau-ops-memory-create-channel-id">Channel</label>
                                <input
                                    id="tau-ops-memory-create-channel-id"
                                    type="text"
                                    name="channel_id"
                                    value=memory_create_channel_id
                                />
                                <label for="tau-ops-memory-create-actor-id">Actor</label>
                                <input
                                    id="tau-ops-memory-create-actor-id"
                                    type="text"
                                    name="actor_id"
                                    value=memory_create_actor_id
                                />
                                <label for="tau-ops-memory-create-memory-type">Memory Type</label>
                                <input
                                    id="tau-ops-memory-create-memory-type"
                                    type="text"
                                    name="memory_type"
                                    value=memory_create_memory_type
                                />
                                <label for="tau-ops-memory-create-importance">Importance</label>
                                <input
                                    id="tau-ops-memory-create-importance"
                                    type="number"
                                    step="0.01"
                                    name="importance"
                                    value=memory_create_importance
                                />
                                <label for="tau-ops-memory-create-relation-target-id">Relation Target</label>
                                <input
                                    id="tau-ops-memory-create-relation-target-id"
                                    type="text"
                                    name="relation_target_id"
                                    value=memory_create_relation_target_id
                                />
                                <label for="tau-ops-memory-create-relation-type">Relation Type</label>
                                <input
                                    id="tau-ops-memory-create-relation-type"
                                    type="text"
                                    name="relation_type"
                                    value=memory_create_relation_type
                                />
                                <label for="tau-ops-memory-create-relation-weight">Relation Weight</label>
                                <input
                                    id="tau-ops-memory-create-relation-weight"
                                    type="number"
                                    step="0.01"
                                    name="relation_weight"
                                    value=memory_create_relation_weight
                                />
                                <button id="tau-ops-memory-create-button" type="submit">
                                    Create Entry
                                </button>
                            </form>
                            <p
                                id="tau-ops-memory-edit-status"
                                data-edit-status=memory_edit_status_marker_attr
                                data-edited-memory-id=memory_edit_edited_memory_id_marker_attr
                            >
                                {memory_edit_status_message}
                            </p>
                            <form
                                id="tau-ops-memory-edit-form"
                                action=memory_edit_form_action
                                method=memory_edit_form_method
                            >
                                <input id="tau-ops-memory-edit-theme" type="hidden" name="theme" value=theme_attr />
                                <input
                                    id="tau-ops-memory-edit-sidebar"
                                    type="hidden"
                                    name="sidebar"
                                    value=sidebar_state_attr
                                />
                                <input
                                    id="tau-ops-memory-edit-session"
                                    type="hidden"
                                    name="session"
                                    value=chat_session_key.clone()
                                />
                                <input
                                    id="tau-ops-memory-edit-operation"
                                    type="hidden"
                                    name="operation"
                                    value="edit"
                                />
                                <label for="tau-ops-memory-edit-entry-id">Entry ID</label>
                                <input
                                    id="tau-ops-memory-edit-entry-id"
                                    type="text"
                                    name="entry_id"
                                    value=memory_edit_entry_id
                                />
                                <label for="tau-ops-memory-edit-summary">Summary</label>
                                <input
                                    id="tau-ops-memory-edit-summary"
                                    type="text"
                                    name="summary"
                                    value=memory_edit_summary
                                />
                                <label for="tau-ops-memory-edit-tags">Tags</label>
                                <input
                                    id="tau-ops-memory-edit-tags"
                                    type="text"
                                    name="tags"
                                    value=memory_edit_tags
                                />
                                <label for="tau-ops-memory-edit-facts">Facts</label>
                                <input
                                    id="tau-ops-memory-edit-facts"
                                    type="text"
                                    name="facts"
                                    value=memory_edit_facts
                                />
                                <label for="tau-ops-memory-edit-source-event-key">Source Event Key</label>
                                <input
                                    id="tau-ops-memory-edit-source-event-key"
                                    type="text"
                                    name="source_event_key"
                                    value=memory_edit_source_event_key
                                />
                                <label for="tau-ops-memory-edit-workspace-id">Workspace</label>
                                <input
                                    id="tau-ops-memory-edit-workspace-id"
                                    type="text"
                                    name="workspace_id"
                                    value=memory_edit_workspace_id
                                />
                                <label for="tau-ops-memory-edit-channel-id">Channel</label>
                                <input
                                    id="tau-ops-memory-edit-channel-id"
                                    type="text"
                                    name="channel_id"
                                    value=memory_edit_channel_id
                                />
                                <label for="tau-ops-memory-edit-actor-id">Actor</label>
                                <input
                                    id="tau-ops-memory-edit-actor-id"
                                    type="text"
                                    name="actor_id"
                                    value=memory_edit_actor_id
                                />
                                <label for="tau-ops-memory-edit-memory-type">Memory Type</label>
                                <input
                                    id="tau-ops-memory-edit-memory-type"
                                    type="text"
                                    name="memory_type"
                                    value=memory_edit_memory_type
                                />
                                <label for="tau-ops-memory-edit-importance">Importance</label>
                                <input
                                    id="tau-ops-memory-edit-importance"
                                    type="number"
                                    step="0.01"
                                    name="importance"
                                    value=memory_edit_importance
                                />
                                <label for="tau-ops-memory-edit-relation-target-id">Relation Target</label>
                                <input
                                    id="tau-ops-memory-edit-relation-target-id"
                                    type="text"
                                    name="relation_target_id"
                                    value=memory_edit_relation_target_id
                                />
                                <label for="tau-ops-memory-edit-relation-type">Relation Type</label>
                                <input
                                    id="tau-ops-memory-edit-relation-type"
                                    type="text"
                                    name="relation_type"
                                    value=memory_edit_relation_type
                                />
                                <label for="tau-ops-memory-edit-relation-weight">Relation Weight</label>
                                <input
                                    id="tau-ops-memory-edit-relation-weight"
                                    type="number"
                                    step="0.01"
                                    name="relation_weight"
                                    value=memory_edit_relation_weight
                                />
                                <button id="tau-ops-memory-edit-button" type="submit">
                                    Update Entry
                                </button>
                            </form>
                            <p
                                id="tau-ops-memory-delete-status"
                                data-delete-status=memory_delete_status_marker_attr
                                data-deleted-memory-id=memory_delete_deleted_entry_id_marker_attr
                            >
                                {memory_delete_status_message}
                            </p>
                            <form
                                id="tau-ops-memory-delete-form"
                                action=memory_delete_form_action
                                method=memory_delete_form_method
                            >
                                <input id="tau-ops-memory-delete-theme" type="hidden" name="theme" value=theme_attr />
                                <input
                                    id="tau-ops-memory-delete-sidebar"
                                    type="hidden"
                                    name="sidebar"
                                    value=sidebar_state_attr
                                />
                                <input
                                    id="tau-ops-memory-delete-session"
                                    type="hidden"
                                    name="session"
                                    value=chat_session_key.clone()
                                />
                                <input
                                    id="tau-ops-memory-delete-operation"
                                    type="hidden"
                                    name="operation"
                                    value="delete"
                                />
                                <label for="tau-ops-memory-delete-entry-id">Entry ID</label>
                                <input
                                    id="tau-ops-memory-delete-entry-id"
                                    type="text"
                                    name="entry_id"
                                    value=memory_delete_entry_id
                                />
                                <label for="tau-ops-memory-delete-confirm">Confirm Delete</label>
                                <input
                                    id="tau-ops-memory-delete-confirm"
                                    type="checkbox"
                                    name="confirm_delete"
                                    value="true"
                                />
                                <button id="tau-ops-memory-delete-button" type="submit">
                                    Delete Entry
                                </button>
                            </form>
                            <section
                                id="tau-ops-memory-detail-panel"
                                data-detail-visible=memory_detail_visible
                                data-memory-id=memory_detail_selected_entry_id
                                data-memory-type=memory_detail_memory_type
                                data-embedding-source=memory_detail_embedding_source_panel_attr
                                data-embedding-model=memory_detail_embedding_model_panel_attr
                                data-embedding-reason-code=memory_detail_embedding_reason_code_panel_attr
                                data-embedding-dimensions=memory_detail_embedding_dimensions_panel_attr
                                data-relation-count=memory_detail_relation_count_panel_attr
                            >
                                <p
                                    id="tau-ops-memory-detail-embedding"
                                    data-embedding-source=memory_detail_embedding_source
                                    data-embedding-model=memory_detail_embedding_model
                                    data-embedding-reason-code=memory_detail_embedding_reason_code
                                    data-embedding-dimensions=memory_detail_embedding_dimensions
                                >
                                    {memory_detail_summary}
                                </p>
                                <ul id="tau-ops-memory-relations" data-relation-count=memory_detail_relation_count>
                                    {memory_detail_relations_view}
                                </ul>
                            </section>
                            <ul id="tau-ops-memory-results" data-result-count=memory_result_count_list_attr>
                                {memory_results_view}
                            </ul>
                        </section>
                        <section
                            id="tau-ops-memory-graph-panel"
                            data-route="/ops/memory-graph"
                            aria-hidden=memory_graph_panel_hidden
                            data-panel-visible=memory_graph_panel_visible
                            data-node-count=memory_graph_node_count_panel_attr
                            data-edge-count=memory_graph_edge_count_panel_attr
                        >
                            <h2>Memory Graph</h2>
                            <ul id="tau-ops-memory-graph-nodes" data-node-count=memory_graph_node_count>
                                {memory_graph_nodes_view}
                            </ul>
                            <ul id="tau-ops-memory-graph-edges" data-edge-count=memory_graph_edge_count>
                                {memory_graph_edges_view}
                            </ul>
                        </section>
                        <section
                            id="tau-ops-command-center"
                            data-route="/ops"
                            aria-hidden=command_center_panel_hidden
                            aria-live="polite"
                        >
                            <section id="tau-ops-kpi-grid" data-kpi-card-count="6">
                                <article
                                    id="tau-ops-kpi-health"
                                    data-component="HealthBadge"
                                    data-health-state=health_state
                                    data-health-reason=health_reason
                                >
                                    <h2>System Health</h2>
                                    <p id="tau-ops-health-state-value">{context.command_center.health_state.clone()}</p>
                                    <p id="tau-ops-health-reason-value">{context.command_center.health_reason.clone()}</p>
                                </article>
                                <article id="tau-ops-kpi-queue-depth" data-component="StatCard" data-kpi-card="queue-depth" data-kpi-value=queue_depth_value>
                                    <h2>Queue Depth</h2>
                                    <p>{context.command_center.queue_depth}</p>
                                </article>
                                <article id="tau-ops-kpi-failure-streak" data-component="StatCard" data-kpi-card="failure-streak" data-kpi-value=failure_streak_value>
                                    <h2>Failure Streak</h2>
                                    <p>{context.command_center.failure_streak}</p>
                                </article>
                                <article id="tau-ops-kpi-processed-cases" data-component="StatCard" data-kpi-card="processed-cases" data-kpi-value=processed_cases_value>
                                    <h2>Processed Cases</h2>
                                    <p>{context.command_center.processed_case_count}</p>
                                </article>
                                <article id="tau-ops-kpi-alert-count" data-component="StatCard" data-kpi-card="alert-count" data-kpi-value=alert_count_value>
                                    <h2>Alert Count</h2>
                                    <p>{context.command_center.alert_count}</p>
                                </article>
                                <article id="tau-ops-kpi-widget-count" data-component="StatCard" data-kpi-card="widget-count" data-kpi-value=widget_count_value>
                                    <h2>Widget Count</h2>
                                    <p>{context.command_center.widget_count}</p>
                                </article>
                                <article id="tau-ops-kpi-timeline-cycles" data-component="StatCard" data-kpi-card="timeline-cycles" data-kpi-value=timeline_cycle_count_value>
                                    <h2>Timeline Cycles</h2>
                                    <p>{context.command_center.timeline_cycle_count}</p>
                                </article>
                            </section>
                            <section
                                id="tau-ops-queue-timeline-chart"
                                data-component="TimelineChart"
                                data-timeline-range=timeline_range.clone()
                                data-timeline-point-count=timeline_point_count_value
                                data-timeline-last-timestamp=timeline_last_timestamp_value
                            >
                                <h2>Queue Timeline</h2>
                                <section
                                    id="tau-ops-timeline-range-controls"
                                    role="group"
                                    aria-label="Timeline range"
                                >
                                    <a
                                        id="tau-ops-timeline-range-1h"
                                        data-range-option="1h"
                                        data-range-selected=range_1h_selected
                                        href=range_1h_href
                                    >
                                        1h
                                    </a>
                                    <a
                                        id="tau-ops-timeline-range-6h"
                                        data-range-option="6h"
                                        data-range-selected=range_6h_selected
                                        href=range_6h_href
                                    >
                                        6h
                                    </a>
                                    <a
                                        id="tau-ops-timeline-range-24h"
                                        data-range-option="24h"
                                        data-range-selected=range_24h_selected
                                        href=range_24h_href
                                    >
                                        24h
                                    </a>
                                </section>
                            </section>
                            <section
                                id="tau-ops-control-panel"
                                data-component="ControlPanel"
                                data-control-mode=control_mode
                                data-rollout-gate=rollout_gate
                                data-control-paused=control_paused_value
                            >
                                <h2>Control State</h2>
                                <section id="tau-ops-control-actions" data-action-count="3">
                                    <button
                                        id="tau-ops-control-action-pause"
                                        data-action-enabled=action_pause_enabled_value
                                        data-action="pause"
                                        data-confirm-required="true"
                                        data-confirm-title="Confirm pause action"
                                        data-confirm-body="Pause command-center processing until resumed."
                                        data-confirm-verb="pause"
                                        type="button"
                                    >
                                        Pause
                                    </button>
                                    <button
                                        id="tau-ops-control-action-resume"
                                        data-action-enabled=action_resume_enabled_value
                                        data-action="resume"
                                        data-confirm-required="true"
                                        data-confirm-title="Confirm resume action"
                                        data-confirm-body="Resume command-center processing."
                                        data-confirm-verb="resume"
                                        type="button"
                                    >
                                        Resume
                                    </button>
                                    <button
                                        id="tau-ops-control-action-refresh"
                                        data-action-enabled=action_refresh_enabled_value
                                        data-action="refresh"
                                        data-confirm-required="true"
                                        data-confirm-title="Confirm refresh action"
                                        data-confirm-body="Refresh command-center state from latest runtime artifacts."
                                        data-confirm-verb="refresh"
                                        type="button"
                                    >
                                        Refresh
                                    </button>
                                </section>
                                <section
                                    id="tau-ops-control-last-action"
                                    data-last-action-request-id=last_action_request_id
                                    data-last-action-name=last_action_name
                                    data-last-action-actor=last_action_actor
                                    data-last-action-timestamp=last_action_timestamp_value
                                >
                                    <h3>Last Action</h3>
                                </section>
                            </section>
                            <section
                                id="tau-ops-alert-feed"
                                data-component="AlertFeed"
                                data-alert-count=alert_count_feed_value
                                data-primary-alert-code=primary_alert_code
                                data-primary-alert-severity=primary_alert_severity
                                data-alert-row-count=alert_row_count_section_value
                            >
                                <h2>Alerts</h2>
                                <p id="tau-ops-primary-alert-message">{primary_alert_message}</p>
                                <ul id="tau-ops-alert-feed-list" data-alert-row-count=alert_row_count_list_value>
                                    {alert_feed_rows
                                        .iter()
                                        .enumerate()
                                        .map(|(index, alert_row)| {
                                            let alert_row_id = format!("tau-ops-alert-row-{index}");
                                            view! {
                                                <li
                                                    id=alert_row_id
                                                    data-alert-code=alert_row.code.clone()
                                                    data-alert-severity=alert_row.severity.clone()
                                                >
                                                    {alert_row.message.clone()}
                                                </li>
                                            }
                                        })
                                        .collect_view()}
                                </ul>
                            </section>
                            <section
                                id="tau-ops-connector-health-table"
                                data-component="ConnectorHealthTable"
                                data-connector-row-count=connector_row_count_table_value
                            >
                                <h2>Connector Health</h2>
                                <table>
                                    <thead>
                                        <tr>
                                            <th scope="col">Channel</th>
                                            <th scope="col">Mode</th>
                                            <th scope="col">Liveness</th>
                                            <th scope="col">Events Ingested</th>
                                            <th scope="col">Provider Failures</th>
                                        </tr>
                                    </thead>
                                    <tbody
                                        id="tau-ops-connector-table-body"
                                        data-connector-row-count=connector_row_count_body_value
                                    >
                                        {connector_health_rows
                                            .iter()
                                            .enumerate()
                                            .map(|(index, connector_row)| {
                                                let connector_row_id =
                                                    format!("tau-ops-connector-row-{index}");
                                                let events_ingested_value =
                                                    connector_row.events_ingested.to_string();
                                                let provider_failures_value =
                                                    connector_row.provider_failures.to_string();
                                                view! {
                                                    <tr
                                                        id=connector_row_id
                                                        data-channel=connector_row.channel.clone()
                                                        data-mode=connector_row.mode.clone()
                                                        data-liveness=connector_row.liveness.clone()
                                                        data-events-ingested=events_ingested_value
                                                        data-provider-failures=provider_failures_value
                                                    >
                                                        <td>{connector_row.channel.clone()}</td>
                                                        <td>{connector_row.mode.clone()}</td>
                                                        <td>{connector_row.liveness.clone()}</td>
                                                        <td>{connector_row.events_ingested}</td>
                                                        <td>{connector_row.provider_failures}</td>
                                                    </tr>
                                                }
                                            })
                                            .collect_view()}
                                    </tbody>
                                </table>
                            </section>
                        <section
                            id="tau-ops-data-table"
                                data-route="/ops"
                                data-timeline-range=timeline_range
                                data-component="DataTable"
                                data-timeline-cycle-count=timeline_cycle_count_table_value
                                data-timeline-invalid-cycle-count=timeline_invalid_cycle_count_value
                            >
                                <h2>Recent Cycles</h2>
                                <table>
                                    <thead>
                                        <tr>
                                            <th scope="col">Last Timestamp</th>
                                            <th scope="col">Point Count</th>
                                            <th scope="col">Cycle Reports</th>
                                            <th scope="col">Invalid Reports</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        <tr
                                            id="tau-ops-timeline-summary-row"
                                            data-row-kind="summary"
                                            data-last-timestamp=timeline_last_timestamp_table_value
                                            data-point-count=timeline_point_count_table_value
                                            data-cycle-count=timeline_cycle_count_summary_value
                                            data-invalid-cycle-count=timeline_invalid_cycle_count_summary_value
                                        >
                                            <td>{context.command_center.timeline_last_timestamp_unix_ms}</td>
                                            <td>{context.command_center.timeline_point_count}</td>
                                            <td>{context.command_center.timeline_cycle_count}</td>
                                            <td>{context.command_center.timeline_invalid_cycle_count}</td>
                                        </tr>
                                        {timeline_empty_row}
                                    </tbody>
                                </table>
                            </section>
                        </section>
                        <section
                            id="tau-ops-deploy-panel"
                            data-route="/ops/deploy"
                            data-component="DeployWizard"
                            aria-hidden=deploy_panel_hidden
                            data-panel-visible=deploy_panel_visible
                        >
                            <h2>Deploy Agent</h2>
                            <nav
                                id="tau-ops-deploy-wizard-steps"
                                data-component="DeployWizardSteps"
                                aria-label="Deploy wizard steps"
                            >
                                <ol>
                                    <li>
                                        <button
                                            type="button"
                                            data-wizard-step="model"
                                            data-step-index="1"
                                        >
                                            "1. Model"
                                        </button>
                                    </li>
                                    <li>
                                        <button
                                            type="button"
                                            data-wizard-step="configuration"
                                            data-step-index="2"
                                        >
                                            "2. Configuration"
                                        </button>
                                    </li>
                                    <li>
                                        <button
                                            type="button"
                                            data-wizard-step="validation"
                                            data-step-index="3"
                                        >
                                            "3. Validation"
                                        </button>
                                    </li>
                                    <li>
                                        <button
                                            type="button"
                                            data-wizard-step="review"
                                            data-step-index="4"
                                        >
                                            "4. Review"
                                        </button>
                                    </li>
                                </ol>
                            </nav>
                            <section id="tau-ops-deploy-model-selection">
                                <label for="tau-ops-deploy-model-catalog">Model Catalog</label>
                                <select
                                    id="tau-ops-deploy-model-catalog"
                                    data-component="ModelCatalogDropdown"
                                >
                                    <option value="gpt-4.1-mini">gpt-4.1-mini</option>
                                    <option value="gpt-4.1">gpt-4.1</option>
                                </select>
                            </section>
                            <section
                                id="tau-ops-deploy-validation"
                                data-component="StepValidation"
                                data-validation-state="pending"
                            >
                                <h3>Validation</h3>
                                <p>Configuration validates on each wizard step.</p>
                            </section>
                            <section id="tau-ops-deploy-review" data-component="DeployReviewSummary">
                                <h3>Review</h3>
                                <p data-field="summary">Pending full configuration summary.</p>
                            </section>
                            <div id="tau-ops-deploy-actions">
                                <button
                                    id="tau-ops-deploy-submit"
                                    type="button"
                                    data-action="deploy-agent"
                                    data-success-redirect-template="/ops/agents/{agent_id}"
                                >
                                    Deploy Agent
                                </button>
                            </div>
                        </section>
                    </main>
                </div>
            </div>
        </div>
    };
    shell.to_html()
}

#[cfg(test)]
mod tests;
