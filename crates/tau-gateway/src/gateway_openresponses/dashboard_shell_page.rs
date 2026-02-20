const DASHBOARD_SHELL_HTML: &str = include_str!("dashboard_shell.html");

pub(super) fn render_gateway_dashboard_shell_page() -> String {
    DASHBOARD_SHELL_HTML.to_string()
}
