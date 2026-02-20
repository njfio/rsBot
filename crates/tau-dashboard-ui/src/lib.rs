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
}

impl Default for TauOpsDashboardShellContext {
    fn default() -> Self {
        Self {
            auth_mode: TauOpsDashboardAuthMode::Token,
            active_route: TauOpsDashboardRoute::Ops,
            theme: TauOpsDashboardTheme::Dark,
            sidebar_state: TauOpsDashboardSidebarState::Expanded,
            command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        }
    }
}

/// Public `fn` `render_tau_ops_dashboard_shell` in `tau-dashboard-ui`.
pub fn render_tau_ops_dashboard_shell() -> String {
    render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext::default())
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
    let timeline_invalid_cycle_count_value = context
        .command_center
        .timeline_invalid_cycle_count
        .to_string();
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
                        <section id="tau-ops-command-center" aria-live="polite">
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
                                data-timeline-range=timeline_range
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
                                        type="button"
                                    >
                                        Pause
                                    </button>
                                    <button
                                        id="tau-ops-control-action-resume"
                                        data-action-enabled=action_resume_enabled_value
                                        data-action="resume"
                                        type="button"
                                    >
                                        Resume
                                    </button>
                                    <button
                                        id="tau-ops-control-action-refresh"
                                        data-action-enabled=action_refresh_enabled_value
                                        data-action="refresh"
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
                                data-component="DataTable"
                                data-timeline-cycle-count=timeline_cycle_count_table_value
                                data-timeline-invalid-cycle-count=timeline_invalid_cycle_count_value
                            >
                                <h2>Recent Cycles</h2>
                                <table>
                                    <thead>
                                        <tr>
                                            <th scope="col">Cycle Reports</th>
                                            <th scope="col">Invalid Reports</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        <tr id="tau-ops-timeline-summary-row">
                                            <td>{context.command_center.timeline_cycle_count}</td>
                                            <td>{context.command_center.timeline_invalid_cycle_count}</td>
                                        </tr>
                                    </tbody>
                                </table>
                            </section>
                        </section>
                    </main>
                </div>
            </div>
        </div>
    };
    shell.to_html()
}

#[cfg(test)]
mod tests {
    use super::{
        render_tau_ops_dashboard_shell, render_tau_ops_dashboard_shell_with_context,
        TauOpsDashboardAlertFeedRow, TauOpsDashboardAuthMode, TauOpsDashboardCommandCenterSnapshot,
        TauOpsDashboardConnectorHealthRow, TauOpsDashboardRoute, TauOpsDashboardShellContext,
        TauOpsDashboardSidebarState, TauOpsDashboardTheme,
    };

    #[test]
    fn functional_render_shell_includes_foundation_markers() {
        let html = render_tau_ops_dashboard_shell();
        assert!(html.contains("id=\"tau-ops-shell\""));
        assert!(html.contains("id=\"tau-ops-header\""));
        assert!(html.contains("id=\"tau-ops-sidebar\""));
        assert!(html.contains("id=\"tau-ops-command-center\""));
    }

    #[test]
    fn regression_render_shell_includes_prd_component_contract_markers() {
        let html = render_tau_ops_dashboard_shell();
        assert!(html.contains("data-component=\"HealthBadge\""));
        assert!(html.contains("data-component=\"StatCard\""));
        assert!(html.contains("data-component=\"AlertFeed\""));
        assert!(html.contains("data-component=\"DataTable\""));
    }

    #[test]
    fn functional_spec_2786_c03_shell_exposes_auth_bootstrap_markers() {
        let html = render_tau_ops_dashboard_shell();
        assert!(html.contains("id=\"tau-ops-auth-shell\""));
        assert!(html.contains("data-auth-mode=\"token\""));
        assert!(html.contains("data-login-required=\"true\""));
        assert!(html.contains("id=\"tau-ops-login-shell\""));
        assert!(html.contains("id=\"tau-ops-protected-shell\""));
    }

    #[test]
    fn conformance_spec_2786_c03_shell_login_route_marks_login_panel_visible() {
        let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
            auth_mode: TauOpsDashboardAuthMode::PasswordSession,
            active_route: TauOpsDashboardRoute::Login,
            theme: TauOpsDashboardTheme::Dark,
            sidebar_state: TauOpsDashboardSidebarState::Expanded,
            command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        });
        assert!(html.contains("data-auth-mode=\"password-session\""));
        assert!(html.contains("data-active-route=\"login\""));
        assert!(html.contains("id=\"tau-ops-login-shell\""));
        assert!(html.contains("aria-hidden=\"false\""));
        assert!(html.contains("id=\"tau-ops-protected-shell\""));
    }

    #[test]
    fn regression_spec_2786_c03_shell_none_mode_marks_auth_not_required() {
        let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
            auth_mode: TauOpsDashboardAuthMode::None,
            active_route: TauOpsDashboardRoute::Ops,
            theme: TauOpsDashboardTheme::Dark,
            sidebar_state: TauOpsDashboardSidebarState::Expanded,
            command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        });
        assert!(html.contains("data-auth-mode=\"none\""));
        assert!(html.contains("data-login-required=\"false\""));
    }

    #[test]
    fn functional_spec_2790_c01_sidebar_includes_14_ops_route_links() {
        let html = render_tau_ops_dashboard_shell();
        assert_eq!(html.matches("data-nav-item=").count(), 14);

        let expected_routes = [
            "/ops",
            "/ops/agents",
            "/ops/agents/default",
            "/ops/chat",
            "/ops/sessions",
            "/ops/memory",
            "/ops/memory-graph",
            "/ops/tools-jobs",
            "/ops/channels",
            "/ops/config",
            "/ops/training",
            "/ops/safety",
            "/ops/diagnostics",
            "/ops/deploy",
        ];

        for route in expected_routes {
            assert!(
                html.contains(&format!("href=\"{route}\"")),
                "missing nav route {route}"
            );
        }
    }

    #[test]
    fn functional_spec_2790_c02_breadcrumb_markers_reflect_ops_route() {
        let html = render_tau_ops_dashboard_shell();
        assert!(html.contains("id=\"tau-ops-breadcrumbs\""));
        assert!(html.contains("data-breadcrumb-current=\"command-center\""));
        assert!(html.contains("id=\"tau-ops-breadcrumb-current\""));
    }

    #[test]
    fn functional_spec_2790_c03_breadcrumb_markers_reflect_login_route() {
        let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
            auth_mode: TauOpsDashboardAuthMode::PasswordSession,
            active_route: TauOpsDashboardRoute::Login,
            theme: TauOpsDashboardTheme::Dark,
            sidebar_state: TauOpsDashboardSidebarState::Expanded,
            command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        });
        assert!(html.contains("id=\"tau-ops-breadcrumbs\""));
        assert!(html.contains("data-breadcrumb-current=\"login\""));
        assert!(html.contains("id=\"tau-ops-breadcrumb-current\""));
    }

    #[test]
    fn functional_spec_2794_c02_c03_route_context_tokens_match_expected_values() {
        let route_cases = [
            (TauOpsDashboardRoute::Ops, "ops", "command-center"),
            (TauOpsDashboardRoute::Agents, "agents", "agent-fleet"),
            (
                TauOpsDashboardRoute::AgentDetail,
                "agent-detail",
                "agent-detail",
            ),
            (TauOpsDashboardRoute::Chat, "chat", "chat"),
            (TauOpsDashboardRoute::Sessions, "sessions", "sessions"),
            (TauOpsDashboardRoute::Memory, "memory", "memory"),
            (
                TauOpsDashboardRoute::MemoryGraph,
                "memory-graph",
                "memory-graph",
            ),
            (TauOpsDashboardRoute::ToolsJobs, "tools-jobs", "tools-jobs"),
            (TauOpsDashboardRoute::Channels, "channels", "channels"),
            (TauOpsDashboardRoute::Config, "config", "config"),
            (TauOpsDashboardRoute::Training, "training", "training"),
            (TauOpsDashboardRoute::Safety, "safety", "safety"),
            (
                TauOpsDashboardRoute::Diagnostics,
                "diagnostics",
                "diagnostics",
            ),
            (TauOpsDashboardRoute::Deploy, "deploy", "deploy"),
            (TauOpsDashboardRoute::Login, "login", "login"),
        ];

        for (route, expected_active_route, expected_breadcrumb) in route_cases {
            let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
                auth_mode: TauOpsDashboardAuthMode::Token,
                active_route: route,
                theme: TauOpsDashboardTheme::Dark,
                sidebar_state: TauOpsDashboardSidebarState::Expanded,
                command_center: TauOpsDashboardCommandCenterSnapshot::default(),
            });
            assert!(html.contains(&format!("data-active-route=\"{expected_active_route}\"")));
            assert!(html.contains(&format!(
                "data-breadcrumb-current=\"{expected_breadcrumb}\""
            )));
        }
    }

    #[test]
    fn functional_spec_2798_c01_c02_c03_shell_exposes_responsive_and_theme_contract_markers() {
        let html = render_tau_ops_dashboard_shell();
        assert!(html.contains("id=\"tau-ops-shell-controls\""));
        assert!(html.contains("id=\"tau-ops-sidebar-toggle\""));
        assert!(html.contains("id=\"tau-ops-sidebar-hamburger\""));
        assert!(html.contains("data-sidebar-mobile-default=\"collapsed\""));
        assert!(html.contains("data-sidebar-state=\"expanded\""));
        assert!(html.contains("data-theme=\"dark\""));
        assert!(html.contains("id=\"tau-ops-theme-toggle-dark\""));
        assert!(html.contains("id=\"tau-ops-theme-toggle-light\""));
    }

    #[test]
    fn functional_spec_2798_c02_shell_sidebar_collapsed_state_updates_toggle_markers() {
        let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
            auth_mode: TauOpsDashboardAuthMode::Token,
            active_route: TauOpsDashboardRoute::Ops,
            theme: TauOpsDashboardTheme::Dark,
            sidebar_state: TauOpsDashboardSidebarState::Collapsed,
            command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        });
        assert!(html.contains("data-sidebar-state=\"collapsed\""));
        assert!(html.contains("data-sidebar-target-state=\"expanded\""));
        assert!(html.contains("aria-expanded=\"false\""));
        assert!(html.contains("href=\"/ops?theme=dark&amp;sidebar=expanded\""));
    }

    #[test]
    fn functional_spec_2798_c03_shell_light_theme_state_updates_theme_markers() {
        let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
            auth_mode: TauOpsDashboardAuthMode::Token,
            active_route: TauOpsDashboardRoute::Chat,
            theme: TauOpsDashboardTheme::Light,
            sidebar_state: TauOpsDashboardSidebarState::Expanded,
            command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        });
        assert!(html.contains("data-theme=\"light\""));
        assert!(html.contains(
            "id=\"tau-ops-theme-toggle-dark\" data-theme-option=\"dark\" aria-pressed=\"false\""
        ));
        assert!(html.contains(
            "id=\"tau-ops-theme-toggle-light\" data-theme-option=\"light\" aria-pressed=\"true\""
        ));
        assert!(html.contains("href=\"/ops/chat?theme=dark&amp;sidebar=expanded\""));
    }

    #[test]
    fn functional_spec_2806_c01_c02_c03_command_center_snapshot_markers_render() {
        let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
            auth_mode: TauOpsDashboardAuthMode::Token,
            active_route: TauOpsDashboardRoute::Ops,
            theme: TauOpsDashboardTheme::Dark,
            sidebar_state: TauOpsDashboardSidebarState::Expanded,
            command_center: TauOpsDashboardCommandCenterSnapshot {
                health_state: "healthy".to_string(),
                health_reason: "no recent transport failures observed".to_string(),
                rollout_gate: "hold".to_string(),
                control_mode: "paused".to_string(),
                control_paused: true,
                action_pause_enabled: false,
                action_resume_enabled: true,
                action_refresh_enabled: true,
                last_action_request_id: "dashboard-action-90210".to_string(),
                last_action_name: "pause".to_string(),
                last_action_actor: "ops-user".to_string(),
                last_action_timestamp_unix_ms: 90210,
                timeline_range: "1h".to_string(),
                timeline_point_count: 9,
                timeline_last_timestamp_unix_ms: 811,
                queue_depth: 3,
                failure_streak: 1,
                processed_case_count: 8,
                alert_count: 2,
                widget_count: 6,
                timeline_cycle_count: 9,
                timeline_invalid_cycle_count: 1,
                primary_alert_code: "dashboard_queue_backlog".to_string(),
                primary_alert_severity: "warning".to_string(),
                primary_alert_message: "runtime backlog detected (queue_depth=3)".to_string(),
                alert_feed_rows: vec![],
                connector_health_rows: vec![],
            },
        });

        assert!(html.contains("data-health-state=\"healthy\""));
        assert!(html.contains("data-health-reason=\"no recent transport failures observed\""));
        assert_eq!(html.matches("data-kpi-card=").count(), 6);
        assert!(html.contains("data-kpi-card=\"queue-depth\" data-kpi-value=\"3\""));
        assert!(html.contains("data-kpi-card=\"failure-streak\" data-kpi-value=\"1\""));
        assert!(html.contains("data-kpi-card=\"processed-cases\" data-kpi-value=\"8\""));
        assert!(html.contains("data-kpi-card=\"alert-count\" data-kpi-value=\"2\""));
        assert!(html.contains("data-kpi-card=\"widget-count\" data-kpi-value=\"6\""));
        assert!(html.contains("data-kpi-card=\"timeline-cycles\" data-kpi-value=\"9\""));
        assert!(html.contains("data-alert-count=\"2\""));
        assert!(html.contains("data-primary-alert-code=\"dashboard_queue_backlog\""));
        assert!(html.contains("data-primary-alert-severity=\"warning\""));
        assert!(html.contains("runtime backlog detected (queue_depth=3)"));
        assert!(html.contains("data-timeline-cycle-count=\"9\""));
        assert!(html.contains("data-timeline-invalid-cycle-count=\"1\""));
        assert!(html.contains("data-control-mode=\"paused\""));
        assert!(html.contains("data-rollout-gate=\"hold\""));
        assert!(html.contains("data-control-paused=\"true\""));
        assert!(html.contains("id=\"tau-ops-control-action-pause\" data-action-enabled=\"false\""));
        assert!(html.contains("id=\"tau-ops-control-action-resume\" data-action-enabled=\"true\""));
        assert!(html.contains("id=\"tau-ops-control-action-refresh\" data-action-enabled=\"true\""));
        assert!(html.contains("data-last-action-request-id=\"dashboard-action-90210\""));
        assert!(html.contains("data-last-action-name=\"pause\""));
        assert!(html.contains("data-last-action-actor=\"ops-user\""));
        assert!(html.contains("data-last-action-timestamp=\"90210\""));
        assert!(html.contains("id=\"tau-ops-queue-timeline-chart\""));
        assert!(html.contains("data-component=\"TimelineChart\""));
        assert!(html.contains("data-timeline-range=\"1h\""));
        assert!(html.contains("data-timeline-point-count=\"9\""));
        assert!(html.contains("data-timeline-last-timestamp=\"811\""));
    }

    #[test]
    fn functional_spec_2810_c01_c02_c03_command_center_control_markers_render() {
        let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
            auth_mode: TauOpsDashboardAuthMode::Token,
            active_route: TauOpsDashboardRoute::Ops,
            theme: TauOpsDashboardTheme::Dark,
            sidebar_state: TauOpsDashboardSidebarState::Expanded,
            command_center: TauOpsDashboardCommandCenterSnapshot {
                health_state: "healthy".to_string(),
                health_reason: "operator pause action is active".to_string(),
                rollout_gate: "hold".to_string(),
                control_mode: "paused".to_string(),
                control_paused: true,
                action_pause_enabled: false,
                action_resume_enabled: true,
                action_refresh_enabled: true,
                last_action_request_id: "dashboard-action-90210".to_string(),
                last_action_name: "pause".to_string(),
                last_action_actor: "ops-user".to_string(),
                last_action_timestamp_unix_ms: 90210,
                timeline_range: "1h".to_string(),
                timeline_point_count: 2,
                timeline_last_timestamp_unix_ms: 811,
                queue_depth: 1,
                failure_streak: 0,
                processed_case_count: 2,
                alert_count: 2,
                widget_count: 2,
                timeline_cycle_count: 2,
                timeline_invalid_cycle_count: 1,
                primary_alert_code: "dashboard_queue_backlog".to_string(),
                primary_alert_severity: "warning".to_string(),
                primary_alert_message: "runtime backlog detected (queue_depth=1)".to_string(),
                alert_feed_rows: vec![],
                connector_health_rows: vec![],
            },
        });

        assert!(html.contains("id=\"tau-ops-control-panel\""));
        assert!(html.contains("data-control-mode=\"paused\""));
        assert!(html.contains("data-rollout-gate=\"hold\""));
        assert!(html.contains("data-control-paused=\"true\""));
        assert!(html.contains("id=\"tau-ops-control-action-pause\" data-action-enabled=\"false\""));
        assert!(html.contains("id=\"tau-ops-control-action-resume\" data-action-enabled=\"true\""));
        assert!(html.contains("id=\"tau-ops-control-action-refresh\" data-action-enabled=\"true\""));
        assert!(html.contains("data-last-action-request-id=\"dashboard-action-90210\""));
        assert!(html.contains("data-last-action-name=\"pause\""));
        assert!(html.contains("data-last-action-actor=\"ops-user\""));
        assert!(html.contains("data-last-action-timestamp=\"90210\""));
    }

    #[test]
    fn functional_spec_2814_c01_c02_c03_timeline_chart_and_range_markers_render() {
        let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
            auth_mode: TauOpsDashboardAuthMode::Token,
            active_route: TauOpsDashboardRoute::Ops,
            theme: TauOpsDashboardTheme::Light,
            sidebar_state: TauOpsDashboardSidebarState::Collapsed,
            command_center: TauOpsDashboardCommandCenterSnapshot {
                health_state: "healthy".to_string(),
                health_reason: "no recent transport failures observed".to_string(),
                rollout_gate: "pass".to_string(),
                control_mode: "running".to_string(),
                control_paused: false,
                action_pause_enabled: true,
                action_resume_enabled: false,
                action_refresh_enabled: true,
                last_action_request_id: "none".to_string(),
                last_action_name: "none".to_string(),
                last_action_actor: "none".to_string(),
                last_action_timestamp_unix_ms: 0,
                timeline_range: "6h".to_string(),
                timeline_point_count: 2,
                timeline_last_timestamp_unix_ms: 811,
                queue_depth: 1,
                failure_streak: 0,
                processed_case_count: 2,
                alert_count: 2,
                widget_count: 2,
                timeline_cycle_count: 2,
                timeline_invalid_cycle_count: 1,
                primary_alert_code: "dashboard_queue_backlog".to_string(),
                primary_alert_severity: "warning".to_string(),
                primary_alert_message: "runtime backlog detected (queue_depth=1)".to_string(),
                alert_feed_rows: vec![],
                connector_health_rows: vec![],
            },
        });

        assert!(html.contains("id=\"tau-ops-queue-timeline-chart\""));
        assert!(html.contains("data-component=\"TimelineChart\""));
        assert!(html.contains("data-timeline-range=\"6h\""));
        assert!(html.contains("data-timeline-point-count=\"2\""));
        assert!(html.contains("data-timeline-last-timestamp=\"811\""));
        assert!(html.contains(
            "id=\"tau-ops-timeline-range-1h\" data-range-option=\"1h\" data-range-selected=\"false\""
        ));
        assert!(html.contains(
            "id=\"tau-ops-timeline-range-6h\" data-range-option=\"6h\" data-range-selected=\"true\""
        ));
        assert!(html.contains(
            "id=\"tau-ops-timeline-range-24h\" data-range-option=\"24h\" data-range-selected=\"false\""
        ));
        assert!(html.contains("href=\"/ops?theme=light&amp;sidebar=collapsed&amp;range=1h\""));
        assert!(html.contains("href=\"/ops?theme=light&amp;sidebar=collapsed&amp;range=6h\""));
        assert!(html.contains("href=\"/ops?theme=light&amp;sidebar=collapsed&amp;range=24h\""));
    }

    #[test]
    fn functional_spec_2818_c01_c02_alert_feed_row_markers_render_for_snapshot_alerts() {
        let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
            auth_mode: TauOpsDashboardAuthMode::Token,
            active_route: TauOpsDashboardRoute::Ops,
            theme: TauOpsDashboardTheme::Dark,
            sidebar_state: TauOpsDashboardSidebarState::Expanded,
            command_center: TauOpsDashboardCommandCenterSnapshot {
                health_state: "degraded".to_string(),
                health_reason: "runtime backlog detected".to_string(),
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
                timeline_point_count: 1,
                timeline_last_timestamp_unix_ms: 900,
                queue_depth: 1,
                failure_streak: 0,
                processed_case_count: 1,
                alert_count: 2,
                widget_count: 1,
                timeline_cycle_count: 1,
                timeline_invalid_cycle_count: 0,
                primary_alert_code: "dashboard_queue_backlog".to_string(),
                primary_alert_severity: "warning".to_string(),
                primary_alert_message: "runtime backlog detected (queue_depth=1)".to_string(),
                alert_feed_rows: vec![
                    TauOpsDashboardAlertFeedRow {
                        code: "dashboard_queue_backlog".to_string(),
                        severity: "warning".to_string(),
                        message: "runtime backlog detected (queue_depth=1)".to_string(),
                    },
                    TauOpsDashboardAlertFeedRow {
                        code: "dashboard_cycle_log_invalid_lines".to_string(),
                        severity: "warning".to_string(),
                        message: "runtime events log contains 1 malformed line(s)".to_string(),
                    },
                ],
                connector_health_rows: vec![],
            },
        });

        assert!(html.contains("id=\"tau-ops-alert-feed-list\""));
        assert!(html.contains(
            "id=\"tau-ops-alert-row-0\" data-alert-code=\"dashboard_queue_backlog\" data-alert-severity=\"warning\""
        ));
        assert!(html.contains(
            "id=\"tau-ops-alert-row-1\" data-alert-code=\"dashboard_cycle_log_invalid_lines\" data-alert-severity=\"warning\""
        ));
        assert!(html.contains("runtime backlog detected (queue_depth=1)"));
    }

    #[test]
    fn functional_spec_2818_c03_alert_feed_row_markers_render_nominal_fallback_alert() {
        let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
            auth_mode: TauOpsDashboardAuthMode::Token,
            active_route: TauOpsDashboardRoute::Ops,
            theme: TauOpsDashboardTheme::Dark,
            sidebar_state: TauOpsDashboardSidebarState::Expanded,
            command_center: TauOpsDashboardCommandCenterSnapshot {
                health_state: "healthy".to_string(),
                health_reason: "dashboard runtime health is nominal".to_string(),
                rollout_gate: "pass".to_string(),
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
                timeline_point_count: 1,
                timeline_last_timestamp_unix_ms: 900,
                queue_depth: 0,
                failure_streak: 0,
                processed_case_count: 1,
                alert_count: 1,
                widget_count: 1,
                timeline_cycle_count: 1,
                timeline_invalid_cycle_count: 0,
                primary_alert_code: "dashboard_healthy".to_string(),
                primary_alert_severity: "info".to_string(),
                primary_alert_message: "dashboard runtime health is nominal".to_string(),
                alert_feed_rows: vec![TauOpsDashboardAlertFeedRow {
                    code: "dashboard_healthy".to_string(),
                    severity: "info".to_string(),
                    message: "dashboard runtime health is nominal".to_string(),
                }],
                connector_health_rows: vec![],
            },
        });

        assert!(html.contains("id=\"tau-ops-alert-feed-list\""));
        assert!(html.contains(
            "id=\"tau-ops-alert-row-0\" data-alert-code=\"dashboard_healthy\" data-alert-severity=\"info\""
        ));
        assert!(html.contains("dashboard runtime health is nominal"));
    }

    #[test]
    fn functional_spec_2822_c03_connector_health_table_renders_fallback_row_markers() {
        let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
            auth_mode: TauOpsDashboardAuthMode::Token,
            active_route: TauOpsDashboardRoute::Ops,
            theme: TauOpsDashboardTheme::Dark,
            sidebar_state: TauOpsDashboardSidebarState::Expanded,
            command_center: TauOpsDashboardCommandCenterSnapshot {
                health_state: "healthy".to_string(),
                health_reason: "dashboard runtime health is nominal".to_string(),
                rollout_gate: "pass".to_string(),
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
                timeline_point_count: 1,
                timeline_last_timestamp_unix_ms: 900,
                queue_depth: 0,
                failure_streak: 0,
                processed_case_count: 1,
                alert_count: 1,
                widget_count: 1,
                timeline_cycle_count: 1,
                timeline_invalid_cycle_count: 0,
                primary_alert_code: "dashboard_healthy".to_string(),
                primary_alert_severity: "info".to_string(),
                primary_alert_message: "dashboard runtime health is nominal".to_string(),
                alert_feed_rows: vec![TauOpsDashboardAlertFeedRow {
                    code: "dashboard_healthy".to_string(),
                    severity: "info".to_string(),
                    message: "dashboard runtime health is nominal".to_string(),
                }],
                connector_health_rows: vec![],
            },
        });

        assert!(html.contains("id=\"tau-ops-connector-health-table\""));
        assert!(html.contains("id=\"tau-ops-connector-table-body\""));
        assert!(html.contains(
            "id=\"tau-ops-connector-row-0\" data-channel=\"none\" data-mode=\"unknown\" data-liveness=\"unknown\" data-events-ingested=\"0\" data-provider-failures=\"0\""
        ));
    }

    #[test]
    fn functional_spec_2822_c01_c02_connector_health_table_rows_render_for_snapshot_connectors() {
        let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
            auth_mode: TauOpsDashboardAuthMode::Token,
            active_route: TauOpsDashboardRoute::Ops,
            theme: TauOpsDashboardTheme::Dark,
            sidebar_state: TauOpsDashboardSidebarState::Expanded,
            command_center: TauOpsDashboardCommandCenterSnapshot {
                health_state: "degraded".to_string(),
                health_reason: "connector retry in progress".to_string(),
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
                timeline_point_count: 1,
                timeline_last_timestamp_unix_ms: 900,
                queue_depth: 0,
                failure_streak: 0,
                processed_case_count: 1,
                alert_count: 1,
                widget_count: 1,
                timeline_cycle_count: 1,
                timeline_invalid_cycle_count: 0,
                primary_alert_code: "dashboard_healthy".to_string(),
                primary_alert_severity: "info".to_string(),
                primary_alert_message: "dashboard runtime health is nominal".to_string(),
                alert_feed_rows: vec![TauOpsDashboardAlertFeedRow {
                    code: "dashboard_healthy".to_string(),
                    severity: "info".to_string(),
                    message: "dashboard runtime health is nominal".to_string(),
                }],
                connector_health_rows: vec![TauOpsDashboardConnectorHealthRow {
                    channel: "telegram".to_string(),
                    mode: "polling".to_string(),
                    liveness: "open".to_string(),
                    events_ingested: 6,
                    provider_failures: 2,
                }],
            },
        });

        assert!(html.contains("id=\"tau-ops-connector-health-table\""));
        assert!(html.contains("id=\"tau-ops-connector-table-body\""));
        assert!(html.contains(
            "id=\"tau-ops-connector-row-0\" data-channel=\"telegram\" data-mode=\"polling\" data-liveness=\"open\" data-events-ingested=\"6\" data-provider-failures=\"2\""
        ));
    }
}
