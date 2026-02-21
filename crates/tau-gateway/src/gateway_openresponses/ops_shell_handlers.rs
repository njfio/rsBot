//! Ops dashboard shell page handler glue.

use super::*;

macro_rules! define_ops_shell_handler {
    ($handler_name:ident, $route:expr) => {
        pub(super) async fn $handler_name(
            State(state): State<Arc<GatewayOpenResponsesServerState>>,
            Query(controls): Query<OpsShellControlsQuery>,
        ) -> Html<String> {
            render_tau_ops_dashboard_shell_for_route(&state, $route, controls, None)
        }
    };
}

define_ops_shell_handler!(handle_ops_dashboard_shell_page, TauOpsDashboardRoute::Ops);
define_ops_shell_handler!(
    handle_ops_dashboard_agents_shell_page,
    TauOpsDashboardRoute::Agents
);
define_ops_shell_handler!(
    handle_ops_dashboard_chat_shell_page,
    TauOpsDashboardRoute::Chat
);
define_ops_shell_handler!(
    handle_ops_dashboard_sessions_shell_page,
    TauOpsDashboardRoute::Sessions
);
define_ops_shell_handler!(
    handle_ops_dashboard_memory_shell_page,
    TauOpsDashboardRoute::Memory
);
define_ops_shell_handler!(
    handle_ops_dashboard_memory_graph_shell_page,
    TauOpsDashboardRoute::MemoryGraph
);
define_ops_shell_handler!(
    handle_ops_dashboard_tools_jobs_shell_page,
    TauOpsDashboardRoute::ToolsJobs
);
define_ops_shell_handler!(
    handle_ops_dashboard_channels_shell_page,
    TauOpsDashboardRoute::Channels
);
define_ops_shell_handler!(
    handle_ops_dashboard_config_shell_page,
    TauOpsDashboardRoute::Config
);
define_ops_shell_handler!(
    handle_ops_dashboard_training_shell_page,
    TauOpsDashboardRoute::Training
);
define_ops_shell_handler!(
    handle_ops_dashboard_safety_shell_page,
    TauOpsDashboardRoute::Safety
);
define_ops_shell_handler!(
    handle_ops_dashboard_diagnostics_shell_page,
    TauOpsDashboardRoute::Diagnostics
);
define_ops_shell_handler!(
    handle_ops_dashboard_deploy_shell_page,
    TauOpsDashboardRoute::Deploy
);
define_ops_shell_handler!(
    handle_ops_dashboard_login_shell_page,
    TauOpsDashboardRoute::Login
);

pub(super) async fn handle_ops_dashboard_agent_detail_shell_page(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    AxumPath(_agent_id): AxumPath<String>,
    Query(controls): Query<OpsShellControlsQuery>,
) -> Html<String> {
    render_tau_ops_dashboard_shell_for_route(
        &state,
        TauOpsDashboardRoute::AgentDetail,
        controls,
        None,
    )
}

pub(super) async fn handle_ops_dashboard_session_detail_shell_page(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    AxumPath(session_key): AxumPath<String>,
    Query(controls): Query<OpsShellControlsQuery>,
) -> Html<String> {
    render_tau_ops_dashboard_shell_for_route(
        &state,
        TauOpsDashboardRoute::Sessions,
        controls,
        Some(session_key.as_str()),
    )
}
