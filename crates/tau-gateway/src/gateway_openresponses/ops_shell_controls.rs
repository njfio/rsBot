use serde::Deserialize;
use tau_dashboard_ui::{TauOpsDashboardSidebarState, TauOpsDashboardTheme};

#[derive(Debug, Clone, Deserialize, Default)]
pub(super) struct OpsShellControlsQuery {
    #[serde(default)]
    theme: String,
    #[serde(default)]
    sidebar: String,
}

impl OpsShellControlsQuery {
    pub(super) fn theme(&self) -> TauOpsDashboardTheme {
        match self.theme.as_str() {
            "light" => TauOpsDashboardTheme::Light,
            _ => TauOpsDashboardTheme::Dark,
        }
    }

    pub(super) fn sidebar_state(&self) -> TauOpsDashboardSidebarState {
        match self.sidebar.as_str() {
            "collapsed" => TauOpsDashboardSidebarState::Collapsed,
            _ => TauOpsDashboardSidebarState::Expanded,
        }
    }
}
