//! Leptos SSR shell foundations for Tau Ops Dashboard.

use leptos::prelude::*;

/// Public `fn` `render_tau_ops_dashboard_shell` in `tau-dashboard-ui`.
pub fn render_tau_ops_dashboard_shell() -> String {
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
                            <li><a href="/dashboard">Legacy Dashboard</a></li>
                            <li><a href="/webchat">Webchat</a></li>
                        </ul>
                    </nav>
                </aside>
                <main id="tau-ops-command-center" aria-live="polite">
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
                </main>
            </div>
        </div>
    };
    shell.to_html()
}

#[cfg(test)]
mod tests {
    use super::render_tau_ops_dashboard_shell;

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
}
