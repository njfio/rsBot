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
/// Public struct `TauOpsDashboardToolInventoryRow` in `tau-dashboard-ui`.
pub struct TauOpsDashboardToolInventoryRow {
    pub tool_name: String,
    pub category: String,
    pub policy: String,
    pub usage_count: u64,
    pub error_rate: String,
    pub avg_latency_ms: String,
    pub last_used_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `TauOpsDashboardToolUsageHistogramRow` in `tau-dashboard-ui`.
pub struct TauOpsDashboardToolUsageHistogramRow {
    pub hour_offset: u8,
    pub call_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `TauOpsDashboardToolInvocationRow` in `tau-dashboard-ui`.
pub struct TauOpsDashboardToolInvocationRow {
    pub timestamp_unix_ms: u64,
    pub args_summary: String,
    pub result_summary: String,
    pub duration_ms: u64,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `TauOpsDashboardJobRow` in `tau-dashboard-ui`.
pub struct TauOpsDashboardJobRow {
    pub job_id: String,
    pub job_name: String,
    pub job_status: String,
    pub started_unix_ms: u64,
    pub finished_unix_ms: u64,
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
    pub memory_graph_zoom_level: String,
    pub memory_graph_pan_x_level: String,
    pub memory_graph_pan_y_level: String,
    pub memory_graph_filter_memory_type: String,
    pub memory_graph_filter_relation_type: String,
    pub memory_graph_node_rows: Vec<TauOpsDashboardMemoryGraphNodeRow>,
    pub memory_graph_edge_rows: Vec<TauOpsDashboardMemoryGraphEdgeRow>,
    pub tools_inventory_rows: Vec<TauOpsDashboardToolInventoryRow>,
    pub tool_detail_selected_tool_name: String,
    pub tool_detail_description: String,
    pub tool_detail_parameter_schema: String,
    pub tool_detail_policy_timeout_ms: u64,
    pub tool_detail_policy_max_output_chars: u64,
    pub tool_detail_policy_sandbox_mode: String,
    pub tool_detail_usage_histogram_rows: Vec<TauOpsDashboardToolUsageHistogramRow>,
    pub tool_detail_recent_invocation_rows: Vec<TauOpsDashboardToolInvocationRow>,
    pub jobs_rows: Vec<TauOpsDashboardJobRow>,
    pub job_detail_selected_job_id: String,
    pub job_detail_status: String,
    pub job_detail_duration_ms: u64,
    pub job_detail_stdout: String,
    pub job_detail_stderr: String,
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
            memory_graph_zoom_level: "1.00".to_string(),
            memory_graph_pan_x_level: "0.00".to_string(),
            memory_graph_pan_y_level: "0.00".to_string(),
            memory_graph_filter_memory_type: "all".to_string(),
            memory_graph_filter_relation_type: "all".to_string(),
            memory_graph_node_rows: vec![],
            memory_graph_edge_rows: vec![],
            tools_inventory_rows: vec![],
            tool_detail_selected_tool_name: String::new(),
            tool_detail_description: String::new(),
            tool_detail_parameter_schema: "{}".to_string(),
            tool_detail_policy_timeout_ms: 120_000,
            tool_detail_policy_max_output_chars: 32_768,
            tool_detail_policy_sandbox_mode: "default".to_string(),
            tool_detail_usage_histogram_rows: vec![],
            tool_detail_recent_invocation_rows: vec![],
            jobs_rows: vec![],
            job_detail_selected_job_id: String::new(),
            job_detail_status: String::new(),
            job_detail_duration_ms: 0,
            job_detail_stdout: String::new(),
            job_detail_stderr: String::new(),
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

fn derive_memory_graph_node_size_contracts(importance: &str) -> (&'static str, String) {
    let normalized_importance = importance
        .parse::<f32>()
        .ok()
        .unwrap_or(0.5)
        .clamp(0.0, 1.0);
    let size_bucket = if normalized_importance < 0.34 {
        "small"
    } else if normalized_importance < 0.67 {
        "medium"
    } else {
        "large"
    };
    let size_px = format!("{:.2}", 12.0 + (normalized_importance * 16.0));
    (size_bucket, size_px)
}

fn derive_memory_graph_node_color_contracts(memory_type: &str) -> (&'static str, &'static str) {
    match memory_type.trim() {
        "goal" => ("goal", "#f59e0b"),
        "fact" => ("fact", "#2563eb"),
        "event" => ("event", "#7c3aed"),
        "observation" => ("observation", "#0d9488"),
        _ => ("unknown", "#6b7280"),
    }
}

fn derive_memory_graph_edge_style_contracts(relation_type: &str) -> (&'static str, &'static str) {
    let normalized = relation_type.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "related_to" | "relates_to" | "supports" | "references" | "part_of" => ("solid", "none"),
        "updates" | "caused_by" | "depends_on" | "result_of" => ("dashed", "6 4"),
        "contradicts" | "blocks" => ("dotted", "2 4"),
        _ => ("solid", "none"),
    }
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
    let tools_panel_hidden = if matches!(context.active_route, TauOpsDashboardRoute::ToolsJobs) {
        "false"
    } else {
        "true"
    };
    let tools_panel_visible = if matches!(context.active_route, TauOpsDashboardRoute::ToolsJobs) {
        "true"
    } else {
        "false"
    };
    let config_panel_hidden = if matches!(context.active_route, TauOpsDashboardRoute::Config) {
        "false"
    } else {
        "true"
    };
    let config_panel_visible = if matches!(context.active_route, TauOpsDashboardRoute::Config) {
        "true"
    } else {
        "false"
    };
    let training_panel_hidden = if matches!(context.active_route, TauOpsDashboardRoute::Training) {
        "false"
    } else {
        "true"
    };
    let training_panel_visible = if matches!(context.active_route, TauOpsDashboardRoute::Training) {
        "true"
    } else {
        "false"
    };
    let safety_panel_hidden = if matches!(context.active_route, TauOpsDashboardRoute::Safety) {
        "false"
    } else {
        "true"
    };
    let safety_panel_visible = if matches!(context.active_route, TauOpsDashboardRoute::Safety) {
        "true"
    } else {
        "false"
    };
    let diagnostics_panel_hidden =
        if matches!(context.active_route, TauOpsDashboardRoute::Diagnostics) {
            "false"
        } else {
            "true"
        };
    let diagnostics_panel_visible =
        if matches!(context.active_route, TauOpsDashboardRoute::Diagnostics) {
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
    let memory_graph_zoom_level_value = context
        .chat
        .memory_graph_zoom_level
        .parse::<f32>()
        .ok()
        .unwrap_or(1.0)
        .clamp(0.25, 2.0);
    let memory_graph_zoom_level = format!("{:.2}", memory_graph_zoom_level_value);
    let memory_graph_zoom_min = "0.25";
    let memory_graph_zoom_max = "2.00";
    let memory_graph_zoom_step = "0.10";
    let memory_graph_zoom_in_level =
        format!("{:.2}", (memory_graph_zoom_level_value + 0.10).min(2.0));
    let memory_graph_zoom_out_level =
        format!("{:.2}", (memory_graph_zoom_level_value - 0.10).max(0.25));
    let memory_graph_pan_x_value = context
        .chat
        .memory_graph_pan_x_level
        .parse::<f32>()
        .ok()
        .unwrap_or(0.0)
        .clamp(-500.0, 500.0);
    let memory_graph_pan_y_value = context
        .chat
        .memory_graph_pan_y_level
        .parse::<f32>()
        .ok()
        .unwrap_or(0.0)
        .clamp(-500.0, 500.0);
    let memory_graph_pan_x_level = format!("{:.2}", memory_graph_pan_x_value);
    let memory_graph_pan_y_level = format!("{:.2}", memory_graph_pan_y_value);
    let memory_graph_pan_step_value = 25.0f32;
    let memory_graph_pan_step = format!("{:.2}", memory_graph_pan_step_value);
    let memory_graph_pan_left_x_level = format!(
        "{:.2}",
        (memory_graph_pan_x_value - memory_graph_pan_step_value).max(-500.0)
    );
    let memory_graph_pan_right_x_level = format!(
        "{:.2}",
        (memory_graph_pan_x_value + memory_graph_pan_step_value).min(500.0)
    );
    let memory_graph_pan_up_y_level = format!(
        "{:.2}",
        (memory_graph_pan_y_value - memory_graph_pan_step_value).max(-500.0)
    );
    let memory_graph_pan_down_y_level = format!(
        "{:.2}",
        (memory_graph_pan_y_value + memory_graph_pan_step_value).min(500.0)
    );
    let memory_graph_filter_memory_type = {
        let value = context.chat.memory_graph_filter_memory_type.trim();
        if value.is_empty() {
            "all".to_string()
        } else {
            value.to_string()
        }
    };
    let memory_graph_filter_relation_type = {
        let value = context.chat.memory_graph_filter_relation_type.trim();
        if value.is_empty() {
            "all".to_string()
        } else {
            value.to_string()
        }
    };
    let filtered_memory_graph_node_rows = if memory_graph_filter_memory_type == "all" {
        memory_graph_node_rows.clone()
    } else {
        memory_graph_node_rows
            .iter()
            .filter(|row| row.memory_type.as_str() == memory_graph_filter_memory_type.as_str())
            .cloned()
            .collect::<Vec<_>>()
    };
    let filtered_memory_graph_node_ids = filtered_memory_graph_node_rows
        .iter()
        .map(|row| row.memory_id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let scope_edges_to_filtered_nodes = memory_graph_filter_memory_type != "all";
    let filtered_memory_graph_edge_rows = memory_graph_edge_rows
        .iter()
        .filter(|row| {
            let within_node_scope = if scope_edges_to_filtered_nodes {
                filtered_memory_graph_node_ids.contains(&row.source_memory_id)
                    && filtered_memory_graph_node_ids.contains(&row.target_memory_id)
            } else {
                true
            };
            (memory_graph_filter_relation_type == "all"
                || row.relation_type.as_str() == memory_graph_filter_relation_type.as_str())
                && within_node_scope
        })
        .cloned()
        .collect::<Vec<_>>();
    let memory_graph_route_href_base = format!(
        "/ops/memory-graph?theme={theme_attr}&sidebar={sidebar_state_attr}&session={chat_session_key}&workspace_id={memory_search_workspace_id}&channel_id={memory_search_channel_id}&actor_id={memory_search_actor_id}&memory_type={memory_search_memory_type}"
    );
    let memory_graph_zoom_in_href = format!(
        "{memory_graph_route_href_base}&graph_zoom={memory_graph_zoom_in_level}&graph_pan_x={memory_graph_pan_x_level}&graph_pan_y={memory_graph_pan_y_level}&graph_filter_memory_type={memory_graph_filter_memory_type}&graph_filter_relation_type={memory_graph_filter_relation_type}"
    );
    let memory_graph_zoom_out_href = format!(
        "{memory_graph_route_href_base}&graph_zoom={memory_graph_zoom_out_level}&graph_pan_x={memory_graph_pan_x_level}&graph_pan_y={memory_graph_pan_y_level}&graph_filter_memory_type={memory_graph_filter_memory_type}&graph_filter_relation_type={memory_graph_filter_relation_type}"
    );
    let memory_graph_pan_left_href = format!(
        "{memory_graph_route_href_base}&graph_zoom={memory_graph_zoom_level}&graph_pan_x={memory_graph_pan_left_x_level}&graph_pan_y={memory_graph_pan_y_level}&graph_filter_memory_type={memory_graph_filter_memory_type}&graph_filter_relation_type={memory_graph_filter_relation_type}"
    );
    let memory_graph_pan_right_href = format!(
        "{memory_graph_route_href_base}&graph_zoom={memory_graph_zoom_level}&graph_pan_x={memory_graph_pan_right_x_level}&graph_pan_y={memory_graph_pan_y_level}&graph_filter_memory_type={memory_graph_filter_memory_type}&graph_filter_relation_type={memory_graph_filter_relation_type}"
    );
    let memory_graph_pan_up_href = format!(
        "{memory_graph_route_href_base}&graph_zoom={memory_graph_zoom_level}&graph_pan_x={memory_graph_pan_x_level}&graph_pan_y={memory_graph_pan_up_y_level}&graph_filter_memory_type={memory_graph_filter_memory_type}&graph_filter_relation_type={memory_graph_filter_relation_type}"
    );
    let memory_graph_pan_down_href = format!(
        "{memory_graph_route_href_base}&graph_zoom={memory_graph_zoom_level}&graph_pan_x={memory_graph_pan_x_level}&graph_pan_y={memory_graph_pan_down_y_level}&graph_filter_memory_type={memory_graph_filter_memory_type}&graph_filter_relation_type={memory_graph_filter_relation_type}"
    );
    let memory_graph_filter_memory_type_all_href = format!(
        "{memory_graph_route_href_base}&graph_zoom={memory_graph_zoom_level}&graph_pan_x={memory_graph_pan_x_level}&graph_pan_y={memory_graph_pan_y_level}&graph_filter_memory_type=all&graph_filter_relation_type={memory_graph_filter_relation_type}"
    );
    let memory_graph_filter_memory_type_goal_href = format!(
        "{memory_graph_route_href_base}&graph_zoom={memory_graph_zoom_level}&graph_pan_x={memory_graph_pan_x_level}&graph_pan_y={memory_graph_pan_y_level}&graph_filter_memory_type=goal&graph_filter_relation_type={memory_graph_filter_relation_type}"
    );
    let memory_graph_filter_relation_type_all_href = format!(
        "{memory_graph_route_href_base}&graph_zoom={memory_graph_zoom_level}&graph_pan_x={memory_graph_pan_x_level}&graph_pan_y={memory_graph_pan_y_level}&graph_filter_memory_type={memory_graph_filter_memory_type}&graph_filter_relation_type=all"
    );
    let memory_graph_filter_relation_type_related_to_href = format!(
        "{memory_graph_route_href_base}&graph_zoom={memory_graph_zoom_level}&graph_pan_x={memory_graph_pan_x_level}&graph_pan_y={memory_graph_pan_y_level}&graph_filter_memory_type={memory_graph_filter_memory_type}&graph_filter_relation_type=related_to"
    );
    let memory_graph_node_count = filtered_memory_graph_node_rows.len().to_string();
    let memory_graph_edge_count = filtered_memory_graph_edge_rows.len().to_string();
    let memory_graph_node_count_panel_attr = memory_graph_node_count.clone();
    let memory_graph_edge_count_panel_attr = memory_graph_edge_count.clone();
    let selected_memory_graph_detail_id = memory_detail_selected_entry_id.clone();
    let memory_graph_node_detail_href_prefix = format!(
        "/ops/memory-graph?theme={theme_attr}&sidebar={sidebar_state_attr}&session={chat_session_key}&workspace_id={memory_search_workspace_id}&channel_id={memory_search_channel_id}&actor_id={memory_search_actor_id}&memory_type={memory_search_memory_type}&detail_memory_id="
    );
    let memory_graph_detail_visible =
        if matches!(context.active_route, TauOpsDashboardRoute::MemoryGraph)
            && context.chat.memory_detail_visible
        {
            "true"
        } else {
            "false"
        };
    let memory_graph_detail_summary = memory_detail_summary.clone();
    let memory_graph_detail_selected_entry_id = memory_detail_selected_entry_id.clone();
    let memory_graph_detail_memory_type = memory_detail_memory_type.clone();
    let memory_graph_detail_relation_count_panel_attr =
        memory_detail_relation_count_panel_attr.clone();
    let memory_graph_detail_open_memory_href = format!(
        "/ops/memory?theme={theme_attr}&sidebar={sidebar_state_attr}&session={chat_session_key}&workspace_id={memory_search_workspace_id}&channel_id={memory_search_channel_id}&actor_id={memory_search_actor_id}&memory_type={memory_search_memory_type}&detail_memory_id={memory_graph_detail_selected_entry_id}"
    );
    let focused_memory_graph_detail_id =
        if memory_graph_detail_visible == "true" && !selected_memory_graph_detail_id.is_empty() {
            Some(selected_memory_graph_detail_id.clone())
        } else {
            None
        };
    let memory_graph_nodes_view = if filtered_memory_graph_node_rows.is_empty() {
        leptos::either::Either::Left(view! {
            <li id="tau-ops-memory-graph-empty-state" data-empty-state="true">
                No memory graph nodes available.
            </li>
        })
    } else {
        leptos::either::Either::Right(
            filtered_memory_graph_node_rows
                .iter()
                .enumerate()
                .map(|(index, row)| {
                    let row_id = format!("tau-ops-memory-graph-node-{index}");
                    let (node_size_bucket, node_size_px) =
                        derive_memory_graph_node_size_contracts(row.importance.as_str());
                    let (node_color_token, node_color_hex) =
                        derive_memory_graph_node_color_contracts(row.memory_type.as_str());
                    let node_selected = if memory_graph_detail_visible == "true"
                        && row.memory_id.as_str() == selected_memory_graph_detail_id.as_str()
                    {
                        "true"
                    } else {
                        "false"
                    };
                    let node_hover_neighbor = if let Some(focused_memory_id) =
                        focused_memory_graph_detail_id.as_deref()
                    {
                        let is_connected_neighbor = row.memory_id.as_str() == focused_memory_id
                            || filtered_memory_graph_edge_rows.iter().any(|edge| {
                                (edge.source_memory_id.as_str() == focused_memory_id
                                    && edge.target_memory_id.as_str() == row.memory_id.as_str())
                                    || (edge.target_memory_id.as_str() == focused_memory_id
                                        && edge.source_memory_id.as_str() == row.memory_id.as_str())
                            });
                        if is_connected_neighbor {
                            "true"
                        } else {
                            "false"
                        }
                    } else {
                        "false"
                    };
                    let node_detail_href =
                        format!("{memory_graph_node_detail_href_prefix}{}", row.memory_id);
                    view! {
                        <li
                            id=row_id
                            data-memory-id=row.memory_id.clone()
                            data-memory-type=row.memory_type.clone()
                            data-importance=row.importance.clone()
                            data-node-size-bucket=node_size_bucket
                            data-node-size-px=node_size_px
                            data-node-color-token=node_color_token
                            data-node-color-hex=node_color_hex
                            data-node-selected=node_selected
                            data-node-hover-neighbor=node_hover_neighbor
                            data-node-detail-target="tau-ops-memory-graph-detail-panel"
                            data-node-detail-href=node_detail_href
                        ></li>
                    }
                })
                .collect_view(),
        )
    };
    let memory_graph_edges_view = filtered_memory_graph_edge_rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let row_id = format!("tau-ops-memory-graph-edge-{index}");
            let (edge_style_token, edge_stroke_dasharray) =
                derive_memory_graph_edge_style_contracts(row.relation_type.as_str());
            let edge_hover_highlighted =
                if let Some(focused_memory_id) = focused_memory_graph_detail_id.as_deref() {
                    if row.source_memory_id.as_str() == focused_memory_id
                        || row.target_memory_id.as_str() == focused_memory_id
                    {
                        "true"
                    } else {
                        "false"
                    }
                } else {
                    "false"
                };
            view! {
                <li
                    id=row_id
                    data-source-memory-id=row.source_memory_id.clone()
                    data-target-memory-id=row.target_memory_id.clone()
                    data-relation-type=row.relation_type.clone()
                    data-relation-weight=row.effective_weight.clone()
                    data-edge-style-token=edge_style_token
                    data-edge-stroke-dasharray=edge_stroke_dasharray
                    data-edge-hover-highlighted=edge_hover_highlighted
                ></li>
            }
        })
        .collect_view();
    let mut tools_inventory_rows = context.chat.tools_inventory_rows.clone();
    tools_inventory_rows.sort_by(|left, right| left.tool_name.cmp(&right.tool_name));
    let tools_total_count_value = tools_inventory_rows.len().to_string();
    let tools_total_count_panel_attr = tools_total_count_value.clone();
    let tools_total_count_summary_attr = tools_total_count_value.clone();
    let tools_row_count_table_attr = tools_total_count_value.clone();
    let tools_row_count_body_attr = tools_total_count_value;
    let tools_jobs_route_active = matches!(context.active_route, TauOpsDashboardRoute::ToolsJobs);
    let tools_inventory_rows_view = if tools_inventory_rows.is_empty() {
        leptos::either::Either::Left(view! {
            <tr id="tau-ops-tools-inventory-empty-state" data-empty-state="true">
                <td colspan="7">No tools registered.</td>
            </tr>
        })
    } else {
        leptos::either::Either::Right(
            tools_inventory_rows
                .iter()
                .enumerate()
                .map(|(index, row)| {
                    let row_id = format!("tau-ops-tools-inventory-row-{index}");
                    let usage_count = row.usage_count.to_string();
                    let last_used_unix_ms = row.last_used_unix_ms.to_string();
                    let usage_count_attr = usage_count.clone();
                    let last_used_unix_ms_attr = last_used_unix_ms.clone();
                    view! {
                        <tr
                            id=row_id
                            data-tool-name=row.tool_name.clone()
                            data-tool-category=row.category.clone()
                            data-tool-policy=row.policy.clone()
                            data-usage-count=usage_count_attr
                            data-error-rate=row.error_rate.clone()
                            data-avg-latency-ms=row.avg_latency_ms.clone()
                            data-last-used-unix-ms=last_used_unix_ms_attr
                        >
                            <td>{row.tool_name.clone()}</td>
                            <td>{row.category.clone()}</td>
                            <td>{row.policy.clone()}</td>
                            <td>{usage_count}</td>
                            <td>{row.error_rate.clone()}</td>
                            <td>{row.avg_latency_ms.clone()}</td>
                            <td>{last_used_unix_ms}</td>
                        </tr>
                    }
                })
                .collect_view(),
        )
    };
    let tool_detail_selected_tool_name = {
        let selected = context.chat.tool_detail_selected_tool_name.trim();
        if !selected.is_empty() {
            selected.to_string()
        } else {
            tools_inventory_rows
                .first()
                .map(|row| row.tool_name.clone())
                .unwrap_or_default()
        }
    };
    let tool_detail_visible =
        if tools_jobs_route_active && !tool_detail_selected_tool_name.is_empty() {
            "true"
        } else {
            "false"
        };
    let tool_detail_description = if context.chat.tool_detail_description.trim().is_empty() {
        "No tool selected.".to_string()
    } else {
        context.chat.tool_detail_description.clone()
    };
    let tool_detail_parameter_schema =
        if context.chat.tool_detail_parameter_schema.trim().is_empty() {
            "{}".to_string()
        } else {
            context.chat.tool_detail_parameter_schema.clone()
        };
    let tool_detail_policy_timeout_ms = context.chat.tool_detail_policy_timeout_ms.to_string();
    let tool_detail_policy_max_output_chars =
        context.chat.tool_detail_policy_max_output_chars.to_string();
    let tool_detail_policy_sandbox_mode = if context.chat.tool_detail_policy_sandbox_mode.is_empty()
    {
        "default".to_string()
    } else {
        context.chat.tool_detail_policy_sandbox_mode.clone()
    };
    let tool_detail_usage_histogram_rows = context.chat.tool_detail_usage_histogram_rows.clone();
    let tool_detail_usage_bucket_count = tool_detail_usage_histogram_rows.len().to_string();
    let tool_detail_usage_histogram_view = if tool_detail_usage_histogram_rows.is_empty() {
        leptos::either::Either::Left(view! {
            <li id="tau-ops-tool-detail-usage-empty-state" data-empty-state="true">
                No usage histogram buckets available.
            </li>
        })
    } else {
        leptos::either::Either::Right(
            tool_detail_usage_histogram_rows
                .iter()
                .enumerate()
                .map(|(index, row)| {
                    let row_id = format!("tau-ops-tool-detail-usage-bucket-{index}");
                    let hour_offset = row.hour_offset.to_string();
                    let call_count = row.call_count.to_string();
                    view! {
                        <li id=row_id data-hour-offset=hour_offset data-call-count=call_count></li>
                    }
                })
                .collect_view(),
        )
    };
    let tool_detail_recent_invocation_rows =
        context.chat.tool_detail_recent_invocation_rows.clone();
    let tool_detail_invocation_count = tool_detail_recent_invocation_rows.len().to_string();
    let tool_detail_invocations_view = if tool_detail_recent_invocation_rows.is_empty() {
        leptos::either::Either::Left(view! {
            <tr id="tau-ops-tool-detail-invocation-empty-state" data-empty-state="true">
                <td colspan="5">No recent invocations recorded.</td>
            </tr>
        })
    } else {
        leptos::either::Either::Right(
            tool_detail_recent_invocation_rows
                .iter()
                .enumerate()
                .map(|(index, row)| {
                    let row_id = format!("tau-ops-tool-detail-invocation-row-{index}");
                    let timestamp_unix_ms = row.timestamp_unix_ms.to_string();
                    let duration_ms = row.duration_ms.to_string();
                    let timestamp_unix_ms_attr = timestamp_unix_ms.clone();
                    let duration_ms_attr = duration_ms.clone();
                    view! {
                        <tr
                            id=row_id
                            data-timestamp-unix-ms=timestamp_unix_ms_attr
                            data-args-summary=row.args_summary.clone()
                            data-result-summary=row.result_summary.clone()
                            data-duration-ms=duration_ms_attr
                            data-status=row.status.clone()
                        >
                            <td>{timestamp_unix_ms}</td>
                            <td>{row.args_summary.clone()}</td>
                            <td>{row.result_summary.clone()}</td>
                            <td>{duration_ms}</td>
                            <td>{row.status.clone()}</td>
                        </tr>
                    }
                })
                .collect_view(),
        )
    };
    let jobs_rows = if tools_jobs_route_active {
        context.chat.jobs_rows.clone()
    } else {
        Vec::new()
    };
    let jobs_total_count_value = jobs_rows.len().to_string();
    let jobs_total_count_panel_attr = jobs_total_count_value.clone();
    let jobs_row_count_table_attr = jobs_total_count_value.clone();
    let jobs_row_count_body_attr = jobs_total_count_value;
    let jobs_running_count = jobs_rows
        .iter()
        .filter(|row| row.job_status.as_str() == "running")
        .count()
        .to_string();
    let jobs_completed_count = jobs_rows
        .iter()
        .filter(|row| row.job_status.as_str() == "completed")
        .count()
        .to_string();
    let jobs_failed_count = jobs_rows
        .iter()
        .filter(|row| row.job_status.as_str() == "failed")
        .count()
        .to_string();
    let jobs_panel_visible = if tools_jobs_route_active {
        "true"
    } else {
        "false"
    };
    let jobs_rows_view = if jobs_rows.is_empty() {
        leptos::either::Either::Left(view! {
            <tr id="tau-ops-jobs-empty-state" data-empty-state="true">
                <td colspan="6">No jobs recorded.</td>
            </tr>
        })
    } else {
        leptos::either::Either::Right(
            jobs_rows
                .iter()
                .enumerate()
                .map(|(index, row)| {
                    let row_id = format!("tau-ops-jobs-row-{index}");
                    let view_output_id = format!("tau-ops-jobs-view-output-{index}");
                    let cancel_id = format!("tau-ops-jobs-cancel-{index}");
                    let job_id = row.job_id.clone();
                    let started_unix_ms = row.started_unix_ms.to_string();
                    let finished_unix_ms = row.finished_unix_ms.to_string();
                    let started_unix_ms_attr = started_unix_ms.clone();
                    let finished_unix_ms_attr = finished_unix_ms.clone();
                    let view_output_href = format!(
                        "{active_shell_path}?theme={theme_attr}&sidebar={sidebar_state_attr}&session={chat_session_key}&job={job_id}"
                    );
                    let cancel_href = format!(
                        "{active_shell_path}?theme={theme_attr}&sidebar={sidebar_state_attr}&session={chat_session_key}&job={job_id}&cancel_job={job_id}"
                    );
                    let cancel_enabled = if row.job_status.as_str() == "running" {
                        "true"
                    } else {
                        "false"
                    };
                    view! {
                        <tr
                            id=row_id
                            data-job-id=row.job_id.clone()
                            data-job-name=row.job_name.clone()
                            data-job-status=row.job_status.clone()
                            data-started-unix-ms=started_unix_ms_attr
                            data-finished-unix-ms=finished_unix_ms_attr
                        >
                            <td>{row.job_id.clone()}</td>
                            <td>{row.job_name.clone()}</td>
                            <td>{row.job_status.clone()}</td>
                            <td>{started_unix_ms}</td>
                            <td>{finished_unix_ms}</td>
                            <td>
                                <a
                                    id=view_output_id
                                    data-action="view-job-output"
                                    data-job-id=row.job_id.clone()
                                    href=view_output_href
                                >
                                    View Output
                                </a>
                                <a
                                    id=cancel_id
                                    data-action="cancel-job"
                                    data-job-id=row.job_id.clone()
                                    data-cancel-enabled=cancel_enabled
                                    href=cancel_href
                                >
                                    Cancel
                                </a>
                            </td>
                        </tr>
                    }
                })
                .collect_view(),
        )
    };
    let job_detail_selected_job_id = {
        let selected = context.chat.job_detail_selected_job_id.trim();
        if !selected.is_empty() {
            selected.to_string()
        } else {
            jobs_rows
                .first()
                .map(|row| row.job_id.clone())
                .unwrap_or_default()
        }
    };
    let selected_job_row = jobs_rows
        .iter()
        .find(|row| row.job_id.as_str() == job_detail_selected_job_id.as_str());
    let job_detail_status = if context.chat.job_detail_status.trim().is_empty() {
        selected_job_row
            .map(|row| row.job_status.clone())
            .unwrap_or_default()
    } else {
        context.chat.job_detail_status.clone()
    };
    let job_detail_duration_ms_value = if context.chat.job_detail_duration_ms == 0 {
        selected_job_row
            .map(|row| row.finished_unix_ms.saturating_sub(row.started_unix_ms))
            .unwrap_or(0)
    } else {
        context.chat.job_detail_duration_ms
    };
    let job_detail_duration_ms = job_detail_duration_ms_value.to_string();
    let job_detail_stdout = context.chat.job_detail_stdout.clone();
    let job_detail_stdout_bytes = job_detail_stdout.len().to_string();
    let job_detail_stderr = context.chat.job_detail_stderr.clone();
    let job_detail_stderr_bytes = job_detail_stderr.len().to_string();
    let job_detail_visible = if tools_jobs_route_active && !job_detail_selected_job_id.is_empty() {
        "true"
    } else {
        "false"
    };
    let job_cancel_status = if job_detail_status.as_str() == "cancelled" {
        "cancelled"
    } else {
        "idle"
    };
    let job_cancel_panel_visible = if tools_jobs_route_active {
        "true"
    } else {
        "false"
    };
    let job_cancel_enabled = if tools_jobs_route_active
        && selected_job_row
            .map(|row| row.job_status.as_str() == "running")
            .unwrap_or(false)
    {
        "true"
    } else {
        "false"
    };
    let job_cancel_submit_href = format!(
        "{active_shell_path}?theme={theme_attr}&sidebar={sidebar_state_attr}&session={chat_session_key}&job={job_detail_selected_job_id}&cancel_job={job_detail_selected_job_id}"
    );
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
    let channels_panel_hidden = if matches!(context.active_route, TauOpsDashboardRoute::Channels) {
        "false"
    } else {
        "true"
    };
    let channels_panel_visible = if matches!(context.active_route, TauOpsDashboardRoute::Channels) {
        "true"
    } else {
        "false"
    };
    let channels_online_count = connector_health_rows
        .iter()
        .filter(|row| matches!(row.liveness.as_str(), "open" | "online"))
        .count()
        .to_string();
    let channels_offline_count = connector_health_rows
        .iter()
        .filter(|row| matches!(row.liveness.as_str(), "offline" | "unknown"))
        .count()
        .to_string();
    let channels_degraded_count = connector_health_rows
        .iter()
        .filter(|row| {
            !matches!(
                row.liveness.as_str(),
                "open" | "online" | "offline" | "unknown"
            )
        })
        .count()
        .to_string();
    let channels_row_count_value = connector_health_rows.len().to_string();
    let channels_row_count_table_value = channels_row_count_value.clone();
    let channels_row_count_body_value = channels_row_count_value.clone();
    let channels_row_count_panel_value = channels_row_count_value;
    let channels_rows_view = connector_health_rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let row_id = format!("tau-ops-channels-row-{index}");
            let login_id = format!("tau-ops-channels-login-{index}");
            let logout_id = format!("tau-ops-channels-logout-{index}");
            let probe_id = format!("tau-ops-channels-probe-{index}");
            let channel = row.channel.clone();
            let liveness = row.liveness.clone();
            let events_ingested = row.events_ingested.to_string();
            let provider_failures = row.provider_failures.to_string();
            let events_ingested_attr = events_ingested.clone();
            let provider_failures_attr = provider_failures.clone();
            let login_enabled = if matches!(liveness.as_str(), "offline" | "unknown") {
                "true"
            } else {
                "false"
            };
            let logout_enabled = if matches!(liveness.as_str(), "open" | "online") {
                "true"
            } else {
                "false"
            };
            let probe_enabled = "true";
            let login_href = format!(
                "{active_shell_path}?theme={theme_attr}&sidebar={sidebar_state_attr}&session={chat_session_key}&channel={channel}&channel_action=login"
            );
            let logout_href = format!(
                "{active_shell_path}?theme={theme_attr}&sidebar={sidebar_state_attr}&session={chat_session_key}&channel={channel}&channel_action=logout"
            );
            let probe_href = format!(
                "{active_shell_path}?theme={theme_attr}&sidebar={sidebar_state_attr}&session={chat_session_key}&channel={channel}&channel_action=probe"
            );
            view! {
                <tr
                    id=row_id
                    data-channel=row.channel.clone()
                    data-mode=row.mode.clone()
                    data-liveness=row.liveness.clone()
                    data-events-ingested=events_ingested_attr
                    data-provider-failures=provider_failures_attr
                >
                    <td>{row.channel.clone()}</td>
                    <td>{row.mode.clone()}</td>
                    <td>{row.liveness.clone()}</td>
                    <td>{events_ingested}</td>
                    <td>{provider_failures}</td>
                    <td>
                        <a
                            id=login_id
                            data-action="channel-login"
                            data-channel=row.channel.clone()
                            data-action-enabled=login_enabled
                            href=login_href
                        >
                            Login
                        </a>
                        <a
                            id=logout_id
                            data-action="channel-logout"
                            data-channel=row.channel.clone()
                            data-action-enabled=logout_enabled
                            href=logout_href
                        >
                            Logout
                        </a>
                        <a
                            id=probe_id
                            data-action="channel-probe"
                            data-channel=row.channel.clone()
                            data-action-enabled=probe_enabled
                            href=probe_href
                        >
                            Probe
                        </a>
                    </td>
                </tr>
            }
        })
        .collect_view();
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
                                data-memory-id=memory_detail_selected_entry_id.clone()
                                data-memory-type=memory_detail_memory_type.clone()
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
                                    {memory_detail_summary.clone()}
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
                            <div
                                id="tau-ops-memory-graph-zoom-controls"
                                data-zoom-level=memory_graph_zoom_level
                                data-zoom-min=memory_graph_zoom_min
                                data-zoom-max=memory_graph_zoom_max
                                data-zoom-step=memory_graph_zoom_step
                            >
                                <a
                                    id="tau-ops-memory-graph-zoom-in"
                                    data-zoom-action="in"
                                    href=memory_graph_zoom_in_href
                                >
                                    Zoom +
                                </a>
                                <a
                                    id="tau-ops-memory-graph-zoom-out"
                                    data-zoom-action="out"
                                    href=memory_graph_zoom_out_href
                                >
                                    Zoom -
                                </a>
                            </div>
                            <div
                                id="tau-ops-memory-graph-pan-controls"
                                data-pan-x=memory_graph_pan_x_level
                                data-pan-y=memory_graph_pan_y_level
                                data-pan-step=memory_graph_pan_step
                            >
                                <a
                                    id="tau-ops-memory-graph-pan-left"
                                    data-pan-action="left"
                                    href=memory_graph_pan_left_href
                                >
                                    Pan Left
                                </a>
                                <a
                                    id="tau-ops-memory-graph-pan-right"
                                    data-pan-action="right"
                                    href=memory_graph_pan_right_href
                                >
                                    Pan Right
                                </a>
                                <a
                                    id="tau-ops-memory-graph-pan-up"
                                    data-pan-action="up"
                                    href=memory_graph_pan_up_href
                                >
                                    Pan Up
                                </a>
                                <a
                                    id="tau-ops-memory-graph-pan-down"
                                    data-pan-action="down"
                                    href=memory_graph_pan_down_href
                                >
                                    Pan Down
                                </a>
                            </div>
                            <div
                                id="tau-ops-memory-graph-filter-controls"
                                data-filter-memory-type=memory_graph_filter_memory_type
                                data-filter-relation-type=memory_graph_filter_relation_type
                            >
                                <a
                                    id="tau-ops-memory-graph-filter-memory-type-all"
                                    data-filter-target="memory-type"
                                    href=memory_graph_filter_memory_type_all_href
                                >
                                    Memory Type: All
                                </a>
                                <a
                                    id="tau-ops-memory-graph-filter-memory-type-goal"
                                    data-filter-target="memory-type"
                                    href=memory_graph_filter_memory_type_goal_href
                                >
                                    Memory Type: Goal
                                </a>
                                <a
                                    id="tau-ops-memory-graph-filter-relation-type-all"
                                    data-filter-target="relation-type"
                                    href=memory_graph_filter_relation_type_all_href
                                >
                                    Relation: All
                                </a>
                                <a
                                    id="tau-ops-memory-graph-filter-relation-type-related-to"
                                    data-filter-target="relation-type"
                                    href=memory_graph_filter_relation_type_related_to_href
                                >
                                    Relation: related_to
                                </a>
                            </div>
                            <section
                                id="tau-ops-memory-graph-detail-panel"
                                data-detail-visible=memory_graph_detail_visible
                                data-memory-id=memory_graph_detail_selected_entry_id.clone()
                                data-memory-type=memory_graph_detail_memory_type
                                data-relation-count=memory_graph_detail_relation_count_panel_attr
                            >
                                <p
                                    id="tau-ops-memory-graph-detail-summary"
                                    data-memory-id=memory_graph_detail_selected_entry_id.clone()
                                >
                                    {memory_graph_detail_summary}
                                </p>
                                <a
                                    id="tau-ops-memory-graph-detail-open-memory"
                                    href=memory_graph_detail_open_memory_href
                                    data-detail-memory-id=memory_graph_detail_selected_entry_id.clone()
                                >
                                    Open in Memory Explorer
                                </a>
                            </section>
                        </section>
                        <section
                            id="tau-ops-tools-panel"
                            data-route="/ops/tools-jobs"
                            aria-hidden=tools_panel_hidden
                            data-panel-visible=tools_panel_visible
                            data-total-tools=tools_total_count_panel_attr
                        >
                            <h2>Tools & Jobs</h2>
                            <p
                                id="tau-ops-tools-inventory-summary"
                                data-total-tools=tools_total_count_summary_attr
                            >
                                Registered tools visible in the current runtime.
                            </p>
                            <table
                                id="tau-ops-tools-inventory-table"
                                data-row-count=tools_row_count_table_attr
                                data-column-count="7"
                            >
                                <thead>
                                    <tr>
                                        <th scope="col">Tool Name</th>
                                        <th scope="col">Category</th>
                                        <th scope="col">Policy</th>
                                        <th scope="col">Usage Count</th>
                                        <th scope="col">Error Rate</th>
                                        <th scope="col">Avg Latency (ms)</th>
                                        <th scope="col">Last Used (unix ms)</th>
                                    </tr>
                                </thead>
                                <tbody
                                    id="tau-ops-tools-inventory-body"
                                    data-row-count=tools_row_count_body_attr
                                >
                                    {tools_inventory_rows_view}
                                </tbody>
                            </table>
                            <section
                                id="tau-ops-tool-detail-panel"
                                data-selected-tool=tool_detail_selected_tool_name.clone()
                                data-detail-visible=tool_detail_visible
                            >
                                <section
                                    id="tau-ops-tool-detail-metadata"
                                    data-tool-name=tool_detail_selected_tool_name.clone()
                                    data-parameter-schema=tool_detail_parameter_schema.clone()
                                >
                                    <p id="tau-ops-tool-detail-description">
                                        {tool_detail_description}
                                    </p>
                                </section>
                                <section
                                    id="tau-ops-tool-detail-policy"
                                    data-timeout-ms=tool_detail_policy_timeout_ms
                                    data-max-output-chars=tool_detail_policy_max_output_chars
                                    data-sandbox-mode=tool_detail_policy_sandbox_mode
                                >
                                    <h3>Policy</h3>
                                </section>
                                <section
                                    id="tau-ops-tool-detail-usage-histogram"
                                    data-bucket-count=tool_detail_usage_bucket_count
                                >
                                    <h3>Usage (24h)</h3>
                                    <ul>{tool_detail_usage_histogram_view}</ul>
                                </section>
                                <section
                                    id="tau-ops-tool-detail-invocations"
                                    data-row-count=tool_detail_invocation_count
                                >
                                    <h3>Recent Invocations</h3>
                                    <table>
                                        <thead>
                                            <tr>
                                                <th scope="col">Timestamp</th>
                                                <th scope="col">Args</th>
                                                <th scope="col">Result</th>
                                                <th scope="col">Duration (ms)</th>
                                                <th scope="col">Status</th>
                                            </tr>
                                        </thead>
                                        <tbody>{tool_detail_invocations_view}</tbody>
                                    </table>
                                </section>
                            </section>
                            <section
                                id="tau-ops-jobs-panel"
                                data-panel-visible=jobs_panel_visible
                                data-total-jobs=jobs_total_count_panel_attr
                            >
                                <h3>Jobs</h3>
                                <p
                                    id="tau-ops-jobs-summary"
                                    data-running-count=jobs_running_count
                                    data-completed-count=jobs_completed_count
                                    data-failed-count=jobs_failed_count
                                >
                                    Running/completed/failed job counts.
                                </p>
                                <table id="tau-ops-jobs-table" data-row-count=jobs_row_count_table_attr>
                                    <thead>
                                        <tr>
                                            <th scope="col">Job ID</th>
                                            <th scope="col">Job Name</th>
                                            <th scope="col">Status</th>
                                            <th scope="col">Started (unix ms)</th>
                                            <th scope="col">Finished (unix ms)</th>
                                            <th scope="col">Actions</th>
                                        </tr>
                                    </thead>
                                    <tbody id="tau-ops-jobs-body" data-row-count=jobs_row_count_body_attr>
                                        {jobs_rows_view}
                                    </tbody>
                                </table>
                            </section>
                            <section
                                id="tau-ops-job-detail-panel"
                                data-selected-job-id=job_detail_selected_job_id.clone()
                                data-detail-visible=job_detail_visible
                            >
                                <section
                                    id="tau-ops-job-detail-metadata"
                                    data-job-id=job_detail_selected_job_id.clone()
                                    data-job-status=job_detail_status.clone()
                                    data-duration-ms=job_detail_duration_ms
                                >
                                    <h4>Selected Job Output</h4>
                                </section>
                                <pre
                                    id="tau-ops-job-detail-stdout"
                                    data-output-bytes=job_detail_stdout_bytes
                                >
                                    {job_detail_stdout}
                                </pre>
                                <pre
                                    id="tau-ops-job-detail-stderr"
                                    data-output-bytes=job_detail_stderr_bytes
                                >
                                    {job_detail_stderr}
                                </pre>
                            </section>
                            <section
                                id="tau-ops-job-cancel-panel"
                                data-requested-job-id=job_detail_selected_job_id.clone()
                                data-cancel-status=job_cancel_status
                                data-panel-visible=job_cancel_panel_visible
                                data-cancel-endpoint-template="/gateway/jobs/{job_id}/cancel"
                            >
                                <a
                                    id="tau-ops-job-cancel-submit"
                                    data-action="cancel-job"
                                    data-job-id=job_detail_selected_job_id.clone()
                                    data-cancel-enabled=job_cancel_enabled
                                    href=job_cancel_submit_href
                                >
                                    Cancel Selected Job
                                </a>
                            </section>
                        </section>
                        <section
                            id="tau-ops-channels-panel"
                            data-route="/ops/channels"
                            aria-hidden=channels_panel_hidden
                            data-panel-visible=channels_panel_visible
                            data-channel-count=channels_row_count_panel_value
                        >
                            <h2>Multi-Channel</h2>
                            <p
                                id="tau-ops-channels-summary"
                                data-online-count=channels_online_count
                                data-offline-count=channels_offline_count
                                data-degraded-count=channels_degraded_count
                            >
                                Channel health summary for all configured connectors.
                            </p>
                            <table id="tau-ops-channels-table" data-row-count=channels_row_count_table_value>
                                <thead>
                                    <tr>
                                        <th scope="col">Channel</th>
                                        <th scope="col">Mode</th>
                                        <th scope="col">Liveness</th>
                                        <th scope="col">Events Ingested</th>
                                        <th scope="col">Provider Failures</th>
                                        <th scope="col">Actions</th>
                                    </tr>
                                </thead>
                                <tbody
                                    id="tau-ops-channels-body"
                                    data-row-count=channels_row_count_body_value
                                >
                                    {channels_rows_view}
                                </tbody>
                            </table>
                        </section>
                        <section
                            id="tau-ops-config-panel"
                            data-route="/ops/config"
                            aria-hidden=config_panel_hidden
                            data-panel-visible=config_panel_visible
                        >
                            <h2>Configuration</h2>
                            <p>Gateway runtime configuration profile and policy contracts.</p>
                            <section
                                id="tau-ops-config-endpoints"
                                data-config-get-endpoint="/gateway/config"
                                data-config-patch-endpoint="/gateway/config"
                            >
                                <h3>Config Endpoints</h3>
                            </section>
                            <section
                                id="tau-ops-config-profile-controls"
                                data-model-ref="gpt-4.1-mini"
                                data-fallback-model-count="2"
                                data-system-prompt-chars="0"
                                data-max-turns="64"
                            >
                                <h3>Profile</h3>
                                <label for="tau-ops-config-profile-model-ref">Model</label>
                                <select
                                    id="tau-ops-config-profile-model-ref"
                                    name="model_ref"
                                    data-control="select"
                                >
                                    <option value="gpt-4.1-mini">gpt-4.1-mini</option>
                                    <option value="gpt-4.1">gpt-4.1</option>
                                </select>
                                <section
                                    id="tau-ops-config-profile-fallback-models"
                                    data-control="ordered-list"
                                >
                                    <h4>Fallback Models</h4>
                                    <ol>
                                        <li data-model-ref="gpt-4.1">gpt-4.1</li>
                                        <li data-model-ref="gpt-4o-mini">gpt-4o-mini</li>
                                    </ol>
                                </section>
                                <label for="tau-ops-config-profile-system-prompt">System Prompt</label>
                                <textarea
                                    id="tau-ops-config-profile-system-prompt"
                                    name="system_prompt"
                                    data-control="textarea"
                                ></textarea>
                                <label for="tau-ops-config-profile-max-turns">Max Turns</label>
                                <input
                                    id="tau-ops-config-profile-max-turns"
                                    name="max_turns"
                                    data-control="number"
                                    type="number"
                                    value="64"
                                />
                            </section>
                            <section
                                id="tau-ops-config-policy-controls"
                                data-tool-policy-preset="balanced"
                                data-bash-profile="balanced"
                                data-os-sandbox-mode="auto"
                            >
                                <h3>Policy</h3>
                                <section
                                    id="tau-ops-config-policy-limits"
                                    data-bash-timeout-ms="120000"
                                    data-max-command-length="8192"
                                    data-max-tool-output-bytes="32768"
                                    data-max-file-read-bytes="262144"
                                    data-max-file-write-bytes="262144"
                                >
                                    <h4>Limits</h4>
                                </section>
                                <section
                                    id="tau-ops-config-policy-heartbeat"
                                    data-runtime-heartbeat-enabled="true"
                                    data-runtime-heartbeat-interval-ms="5000"
                                    data-runtime-self-repair-enabled="true"
                                >
                                    <h4>Heartbeat</h4>
                                </section>
                                <section
                                    id="tau-ops-config-policy-compaction"
                                    data-warn-threshold="70"
                                    data-aggressive-threshold="85"
                                    data-emergency-threshold="95"
                                >
                                    <h4>Compaction Thresholds</h4>
                                </section>
                            </section>
                        </section>
                        <section
                            id="tau-ops-training-panel"
                            data-route="/ops/training"
                            aria-hidden=training_panel_hidden
                            data-panel-visible=training_panel_visible
                        >
                            <h2>Training & RL</h2>
                            <p>Training status, rollout history, optimizer, and controls.</p>
                            <section
                                id="tau-ops-training-status"
                                data-status="running"
                                data-gate=context.command_center.rollout_gate.clone()
                                data-store-path=".tau/training/rl.sqlite"
                                data-update-interval-rollouts="8"
                                data-max-rollouts-per-update="64"
                                data-failure-streak="0/3"
                            >
                                <h3>Status</h3>
                            </section>
                            <section
                                id="tau-ops-training-rollouts"
                                data-rollout-count="3"
                                data-last-rollout-id="142"
                            >
                                <h3>Rollout History</h3>
                                <ol>
                                    <li data-rollout-id="142" data-steps="12" data-reward="+0.8" data-outcome="completed">#142</li>
                                    <li data-rollout-id="141" data-steps="8" data-reward="+0.5" data-outcome="completed">#141</li>
                                    <li data-rollout-id="140" data-steps="15" data-reward="-0.2" data-outcome="failed">#140</li>
                                </ol>
                            </section>
                            <section
                                id="tau-ops-training-optimizer"
                                data-mean-total-loss="0.023"
                                data-approx-kl="0.0012"
                                data-early-stop="false"
                            >
                                <h3>Optimizer Report</h3>
                            </section>
                            <section
                                id="tau-ops-training-endpoints"
                                data-training-status-endpoint="/gateway/training/status"
                                data-training-rollouts-endpoint="/gateway/training/rollouts"
                                data-training-config-endpoint="/gateway/training/config"
                            >
                                <h3>Training Endpoints</h3>
                            </section>
                            <section
                                id="tau-ops-training-actions"
                                data-pause-endpoint="/gateway/training/config"
                                data-reset-endpoint="/gateway/training/config"
                                data-export-endpoint="/gateway/training/rollouts"
                            >
                                <a
                                    id="tau-ops-training-action-pause"
                                    data-action="pause-training"
                                    data-action-enabled="true"
                                    href="/ops/training?action=pause"
                                >
                                    Pause Training
                                </a>
                                <a
                                    id="tau-ops-training-action-reset"
                                    data-action="reset-store"
                                    data-action-enabled="true"
                                    href="/ops/training?action=reset"
                                >
                                    Reset Store
                                </a>
                                <a
                                    id="tau-ops-training-action-export"
                                    data-action="export-data"
                                    data-action-enabled="true"
                                    href="/ops/training?action=export"
                                >
                                    Export Data
                                </a>
                            </section>
                        </section>
                        <section
                            id="tau-ops-safety-panel"
                            data-route="/ops/safety"
                            aria-hidden=safety_panel_hidden
                            data-panel-visible=safety_panel_visible
                        >
                            <h2>Safety & Security</h2>
                            <p>Safety policy/rules contract endpoints.</p>
                            <section
                                id="tau-ops-safety-endpoints"
                                data-safety-policy-get-endpoint="/gateway/safety/policy"
                                data-safety-policy-put-endpoint="/gateway/safety/policy"
                                data-safety-rules-get-endpoint="/gateway/safety/rules"
                                data-safety-rules-put-endpoint="/gateway/safety/rules"
                                data-safety-test-endpoint="/gateway/safety/test"
                            >
                                <h3>Safety Endpoints</h3>
                            </section>
                        </section>
                        <section
                            id="tau-ops-diagnostics-panel"
                            data-route="/ops/diagnostics"
                            aria-hidden=diagnostics_panel_hidden
                            data-panel-visible=diagnostics_panel_visible
                        >
                            <h2>Diagnostics & Audit</h2>
                            <p>Audit and telemetry contract endpoints.</p>
                            <section
                                id="tau-ops-diagnostics-endpoints"
                                data-audit-summary-endpoint="/gateway/audit/summary"
                                data-audit-log-endpoint="/gateway/audit/log"
                                data-ui-telemetry-endpoint="/gateway/ui/telemetry"
                            >
                                <h3>Diagnostics Endpoints</h3>
                            </section>
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
