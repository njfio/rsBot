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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Public struct `TauOpsDashboardShellContext` in `tau-dashboard-ui`.
pub struct TauOpsDashboardShellContext {
    pub auth_mode: TauOpsDashboardAuthMode,
    pub active_route: TauOpsDashboardRoute,
    pub theme: TauOpsDashboardTheme,
    pub sidebar_state: TauOpsDashboardSidebarState,
}

impl Default for TauOpsDashboardShellContext {
    fn default() -> Self {
        Self {
            auth_mode: TauOpsDashboardAuthMode::Token,
            active_route: TauOpsDashboardRoute::Ops,
            theme: TauOpsDashboardTheme::Dark,
            sidebar_state: TauOpsDashboardSidebarState::Expanded,
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
                            <section id="tau-ops-kpi-grid">
                                <article id="tau-ops-kpi-health" data-component="HealthBadge">
                                    <h2>System Health</h2>
                                    <p>Awaiting live gateway data</p>
                                </article>
                                <article id="tau-ops-kpi-queue" data-component="StatCard">
                                    <h2>Queue Depth</h2>
                                    <p>Awaiting live gateway data</p>
                                </article>
                            </section>
                            <section id="tau-ops-alert-feed" data-component="AlertFeed">
                                <h2>Alerts</h2>
                                <p>No alerts loaded</p>
                            </section>
                            <section id="tau-ops-data-table" data-component="DataTable">
                                <h2>Recent Cycles</h2>
                                <table>
                                    <thead>
                                        <tr>
                                            <th scope="col">Cycle</th>
                                            <th scope="col">Status</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        <tr>
                                            <td>bootstrap</td>
                                            <td>pending</td>
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
        TauOpsDashboardAuthMode, TauOpsDashboardRoute, TauOpsDashboardShellContext,
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
}
