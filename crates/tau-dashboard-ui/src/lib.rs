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
    Login,
}

impl TauOpsDashboardRoute {
    /// Public `fn` `as_str` in `tau-dashboard-ui`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ops => "ops",
            Self::Login => "login",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Public struct `TauOpsDashboardShellContext` in `tau-dashboard-ui`.
pub struct TauOpsDashboardShellContext {
    pub auth_mode: TauOpsDashboardAuthMode,
    pub active_route: TauOpsDashboardRoute,
}

impl Default for TauOpsDashboardShellContext {
    fn default() -> Self {
        Self {
            auth_mode: TauOpsDashboardAuthMode::Token,
            active_route: TauOpsDashboardRoute::Ops,
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
    let login_hidden = if matches!(context.active_route, TauOpsDashboardRoute::Login) {
        "false"
    } else {
        "true"
    };
    let protected_hidden = if matches!(context.active_route, TauOpsDashboardRoute::Ops) {
        "false"
    } else {
        "true"
    };

    let shell = view! {
        <div id="tau-ops-shell" data-app="tau-ops-dashboard">
            <header id="tau-ops-header">
                <h1>Tau Ops Dashboard</h1>
                <p>Leptos SSR foundation shell</p>
            </header>
            <div id="tau-ops-layout">
                <aside id="tau-ops-sidebar">
                    <nav aria-label="Tau Ops navigation">
                        <ul>
                            <li><a href="/ops">Command Center</a></li>
                            <li><a href="/ops/login">Operator Login</a></li>
                            <li><a href="/dashboard">Legacy Dashboard</a></li>
                            <li><a href="/webchat">Webchat</a></li>
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
        });
        assert!(html.contains("data-auth-mode=\"none\""));
        assert!(html.contains("data-login-required=\"false\""));
    }
}
