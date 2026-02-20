//! Leptos SSR shell foundations for Tau Ops Dashboard.

use leptos::prelude::*;

/// Public `fn` `render_tau_ops_dashboard_shell` in `tau-dashboard-ui`.
pub fn render_tau_ops_dashboard_shell() -> String {
    render_tau_ops_dashboard_shell_for_route("/ops")
}

/// Route-aware SSR shell renderer used for contract-level route panel testing.
pub fn render_tau_ops_dashboard_shell_for_route(route: &str) -> String {
    let is_deploy_route = route == "/ops/deploy";
    let shell = view! {
        <div id="tau-ops-shell" data-app="tau-ops-dashboard" data-route=route>
            <header id="tau-ops-header">
                <h1>Tau Ops Dashboard</h1>
                <p>Leptos SSR foundation shell</p>
            </header>
            <div id="tau-ops-layout">
                <aside id="tau-ops-sidebar">
                    <nav aria-label="Tau Ops navigation">
                        <ul>
                            <li><a href="/ops">Command Center</a></li>
                            <li><a href="/ops/deploy">Deploy Agent</a></li>
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
                    {if is_deploy_route {
                        view! {
                            <section id="tau-ops-deploy-panel" data-component="DeployWizard" data-route="/ops/deploy">
                                <h2>Deploy Agent</h2>
                                <nav id="tau-ops-deploy-wizard-steps" data-component="DeployWizardSteps" aria-label="Deploy wizard steps">
                                    <ol>
                                        <li>
                                            <button type="button" data-wizard-step="model" data-step-index="1">
                                                "1. Model"
                                            </button>
                                        </li>
                                        <li>
                                            <button type="button" data-wizard-step="configuration" data-step-index="2">
                                                "2. Configuration"
                                            </button>
                                        </li>
                                        <li>
                                            <button type="button" data-wizard-step="validation" data-step-index="3">
                                                "3. Validation"
                                            </button>
                                        </li>
                                        <li>
                                            <button type="button" data-wizard-step="review" data-step-index="4">
                                                "4. Review"
                                            </button>
                                        </li>
                                    </ol>
                                </nav>
                                <section id="tau-ops-deploy-model-selection">
                                    <label for="tau-ops-deploy-model-catalog">Model Catalog</label>
                                    <select id="tau-ops-deploy-model-catalog" data-component="ModelCatalogDropdown">
                                        <option value="gpt-4.1-mini">gpt-4.1-mini</option>
                                        <option value="gpt-4.1">gpt-4.1</option>
                                    </select>
                                </section>
                                <section id="tau-ops-deploy-validation" data-component="StepValidation" data-validation-state="pending">
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
                        }
                            .into_any()
                    } else {
                        ().into_any()
                    }}
                </main>
            </div>
        </div>
    };
    shell.to_html()
}

#[cfg(test)]
mod tests {
    use super::{render_tau_ops_dashboard_shell, render_tau_ops_dashboard_shell_for_route};

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
    fn spec_c01_deploy_route_renders_wizard_root_and_steps() {
        let html = render_tau_ops_dashboard_shell_for_route("/ops/deploy");
        assert!(html.contains("id=\"tau-ops-deploy-panel\""));
        assert!(html.contains("id=\"tau-ops-deploy-wizard-steps\""));
        assert!(html.contains("data-wizard-step=\"model\""));
        assert!(html.contains("data-wizard-step=\"review\""));
    }

    #[test]
    fn spec_c02_deploy_route_renders_model_catalog_marker() {
        let html = render_tau_ops_dashboard_shell_for_route("/ops/deploy");
        assert!(html.contains("id=\"tau-ops-deploy-model-catalog\""));
        assert!(html.contains("data-component=\"ModelCatalogDropdown\""));
    }

    #[test]
    fn spec_c03_deploy_route_renders_validation_and_review_markers() {
        let html = render_tau_ops_dashboard_shell_for_route("/ops/deploy");
        assert!(html.contains("id=\"tau-ops-deploy-validation\""));
        assert!(html.contains("data-component=\"StepValidation\""));
        assert!(html.contains("id=\"tau-ops-deploy-review\""));
        assert!(html.contains("data-component=\"DeployReviewSummary\""));
    }

    #[test]
    fn spec_c04_deploy_route_renders_deploy_action_marker() {
        let html = render_tau_ops_dashboard_shell_for_route("/ops/deploy");
        assert!(html.contains("id=\"tau-ops-deploy-submit\""));
        assert!(html.contains("data-action=\"deploy-agent\""));
        assert!(html.contains("data-success-redirect-template=\"/ops/agents/{agent_id}\""));
    }

    #[test]
    fn spec_c05_non_deploy_route_hides_deploy_panel_markers() {
        let html = render_tau_ops_dashboard_shell_for_route("/ops");
        assert!(!html.contains("id=\"tau-ops-deploy-panel\""));
        assert!(!html.contains("id=\"tau-ops-deploy-wizard-steps\""));
        assert!(!html.contains("id=\"tau-ops-deploy-submit\""));
    }
}
