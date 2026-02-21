use serde::Deserialize;
use tau_dashboard_ui::{TauOpsDashboardSidebarState, TauOpsDashboardTheme};
use tau_memory::runtime::MemoryType;

#[derive(Debug, Clone, Deserialize, Default)]
pub(super) struct OpsShellControlsQuery {
    #[serde(default)]
    theme: String,
    #[serde(default)]
    sidebar: String,
    #[serde(default)]
    range: String,
    #[serde(default)]
    session_key: String,
    #[serde(default)]
    session: String,
    #[serde(default)]
    query: String,
    #[serde(default)]
    workspace_id: String,
    #[serde(default)]
    channel_id: String,
    #[serde(default)]
    actor_id: String,
    #[serde(default)]
    limit: String,
    #[serde(default)]
    memory_type: String,
    #[serde(default)]
    create_status: String,
    #[serde(default)]
    created_memory_id: String,
    #[serde(default)]
    delete_status: String,
    #[serde(default)]
    deleted_memory_id: String,
    #[serde(default)]
    detail_memory_id: String,
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

    pub(super) fn timeline_range(&self) -> &'static str {
        match self.range.as_str() {
            "6h" => "6h",
            "24h" => "24h",
            _ => "1h",
        }
    }

    pub(super) fn requested_session_key(&self) -> Option<&str> {
        let session_key = if self.session_key.trim().is_empty() {
            self.session.trim()
        } else {
            self.session_key.trim()
        };
        if session_key.is_empty() {
            None
        } else {
            Some(session_key)
        }
    }

    pub(super) fn requested_memory_query(&self) -> Option<&str> {
        let query = self.query.trim();
        if query.is_empty() {
            None
        } else {
            Some(query)
        }
    }

    pub(super) fn requested_memory_workspace_id(&self) -> Option<String> {
        let value = self.workspace_id.trim();
        if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        }
    }

    pub(super) fn requested_memory_channel_id(&self) -> Option<String> {
        let value = self.channel_id.trim();
        if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        }
    }

    pub(super) fn requested_memory_actor_id(&self) -> Option<String> {
        let value = self.actor_id.trim();
        if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        }
    }

    pub(super) fn requested_memory_limit(&self) -> usize {
        self.limit
            .trim()
            .parse::<usize>()
            .ok()
            .map(|value| value.clamp(1, 25))
            .unwrap_or(25)
    }

    pub(super) fn requested_memory_type(&self) -> Option<String> {
        let value = self.memory_type.trim();
        if value.is_empty() {
            return None;
        }
        MemoryType::parse(value).map(|memory_type| memory_type.as_str().to_string())
    }

    pub(super) fn requested_memory_create_status(&self) -> &'static str {
        match self.create_status.trim() {
            "created" => "created",
            "updated" => "updated",
            _ => "idle",
        }
    }

    pub(super) fn requested_memory_created_entry_id(&self) -> Option<String> {
        let value = self.created_memory_id.trim();
        if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        }
    }

    pub(super) fn requested_memory_delete_status(&self) -> &'static str {
        match self.delete_status.trim() {
            "deleted" => "deleted",
            _ => "idle",
        }
    }

    pub(super) fn requested_memory_deleted_entry_id(&self) -> Option<String> {
        let value = self.deleted_memory_id.trim();
        if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        }
    }

    pub(super) fn requested_memory_detail_entry_id(&self) -> Option<String> {
        let value = self.detail_memory_id.trim();
        if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::OpsShellControlsQuery;

    #[test]
    fn unit_timeline_range_returns_selected_supported_values() {
        let six_hours = OpsShellControlsQuery {
            range: "6h".to_string(),
            ..OpsShellControlsQuery::default()
        };
        assert_eq!(six_hours.timeline_range(), "6h");

        let twenty_four_hours = OpsShellControlsQuery {
            range: "24h".to_string(),
            ..OpsShellControlsQuery::default()
        };
        assert_eq!(twenty_four_hours.timeline_range(), "24h");
    }

    #[test]
    fn unit_timeline_range_defaults_to_one_hour_for_invalid_values() {
        let invalid = OpsShellControlsQuery {
            range: "quarter".to_string(),
            ..OpsShellControlsQuery::default()
        };
        assert_eq!(invalid.timeline_range(), "1h");

        let empty = OpsShellControlsQuery::default();
        assert_eq!(empty.timeline_range(), "1h");
    }

    #[test]
    fn unit_requested_session_key_prefers_explicit_session_key_over_session_alias() {
        let controls = OpsShellControlsQuery {
            session_key: "priority-key".to_string(),
            session: "fallback-key".to_string(),
            ..OpsShellControlsQuery::default()
        };
        assert_eq!(controls.requested_session_key(), Some("priority-key"));
    }

    #[test]
    fn unit_requested_session_key_returns_none_when_both_inputs_empty() {
        let controls = OpsShellControlsQuery::default();
        assert_eq!(controls.requested_session_key(), None);
    }

    #[test]
    fn unit_requested_memory_query_returns_trimmed_query_when_present() {
        let controls = OpsShellControlsQuery {
            query: " ArcSwap ".to_string(),
            ..OpsShellControlsQuery::default()
        };
        assert_eq!(controls.requested_memory_query(), Some("ArcSwap"));
    }

    #[test]
    fn unit_requested_memory_workspace_id_trims_and_normalizes_empty_values() {
        let controls = OpsShellControlsQuery {
            workspace_id: " workspace-a ".to_string(),
            ..OpsShellControlsQuery::default()
        };
        assert_eq!(
            controls.requested_memory_workspace_id().as_deref(),
            Some("workspace-a")
        );

        let empty = OpsShellControlsQuery::default();
        assert_eq!(empty.requested_memory_workspace_id(), None);
    }

    #[test]
    fn unit_requested_memory_channel_id_trims_and_normalizes_empty_values() {
        let controls = OpsShellControlsQuery {
            channel_id: " channel-a ".to_string(),
            ..OpsShellControlsQuery::default()
        };
        assert_eq!(
            controls.requested_memory_channel_id().as_deref(),
            Some("channel-a")
        );

        let empty = OpsShellControlsQuery::default();
        assert_eq!(empty.requested_memory_channel_id(), None);
    }

    #[test]
    fn unit_requested_memory_actor_id_trims_and_normalizes_empty_values() {
        let controls = OpsShellControlsQuery {
            actor_id: " operator ".to_string(),
            ..OpsShellControlsQuery::default()
        };
        assert_eq!(
            controls.requested_memory_actor_id().as_deref(),
            Some("operator")
        );

        let empty = OpsShellControlsQuery::default();
        assert_eq!(empty.requested_memory_actor_id(), None);
    }

    #[test]
    fn unit_requested_memory_limit_parses_and_clamps_supported_values() {
        let valid = OpsShellControlsQuery {
            limit: "7".to_string(),
            ..OpsShellControlsQuery::default()
        };
        assert_eq!(valid.requested_memory_limit(), 7);

        let too_large = OpsShellControlsQuery {
            limit: "250".to_string(),
            ..OpsShellControlsQuery::default()
        };
        assert_eq!(too_large.requested_memory_limit(), 25);

        let invalid = OpsShellControlsQuery {
            limit: "not-a-number".to_string(),
            ..OpsShellControlsQuery::default()
        };
        assert_eq!(invalid.requested_memory_limit(), 25);
    }

    #[test]
    fn unit_requested_memory_type_normalizes_supported_values() {
        let valid = OpsShellControlsQuery {
            memory_type: " Goal ".to_string(),
            ..OpsShellControlsQuery::default()
        };
        assert_eq!(valid.requested_memory_type().as_deref(), Some("goal"));

        let invalid = OpsShellControlsQuery {
            memory_type: "unsupported".to_string(),
            ..OpsShellControlsQuery::default()
        };
        assert_eq!(invalid.requested_memory_type(), None);
    }

    #[test]
    fn unit_requested_memory_create_status_defaults_to_idle_and_accepts_known_states() {
        let idle = OpsShellControlsQuery::default();
        assert_eq!(idle.requested_memory_create_status(), "idle");

        let created = OpsShellControlsQuery {
            create_status: "created".to_string(),
            ..OpsShellControlsQuery::default()
        };
        assert_eq!(created.requested_memory_create_status(), "created");

        let updated = OpsShellControlsQuery {
            create_status: "updated".to_string(),
            ..OpsShellControlsQuery::default()
        };
        assert_eq!(updated.requested_memory_create_status(), "updated");

        let invalid = OpsShellControlsQuery {
            create_status: "invalid".to_string(),
            ..OpsShellControlsQuery::default()
        };
        assert_eq!(invalid.requested_memory_create_status(), "idle");
    }

    #[test]
    fn unit_requested_memory_created_entry_id_trims_and_normalizes_empty_values() {
        let valid = OpsShellControlsQuery {
            created_memory_id: " mem-create-1 ".to_string(),
            ..OpsShellControlsQuery::default()
        };
        assert_eq!(
            valid.requested_memory_created_entry_id().as_deref(),
            Some("mem-create-1")
        );

        let empty = OpsShellControlsQuery::default();
        assert_eq!(empty.requested_memory_created_entry_id(), None);
    }

    #[test]
    fn unit_requested_memory_delete_status_defaults_to_idle_and_accepts_deleted() {
        let idle = OpsShellControlsQuery::default();
        assert_eq!(idle.requested_memory_delete_status(), "idle");

        let deleted = OpsShellControlsQuery {
            delete_status: "deleted".to_string(),
            ..OpsShellControlsQuery::default()
        };
        assert_eq!(deleted.requested_memory_delete_status(), "deleted");

        let invalid = OpsShellControlsQuery {
            delete_status: "invalid".to_string(),
            ..OpsShellControlsQuery::default()
        };
        assert_eq!(invalid.requested_memory_delete_status(), "idle");
    }

    #[test]
    fn unit_requested_memory_deleted_entry_id_trims_and_normalizes_empty_values() {
        let valid = OpsShellControlsQuery {
            deleted_memory_id: " mem-delete-1 ".to_string(),
            ..OpsShellControlsQuery::default()
        };
        assert_eq!(
            valid.requested_memory_deleted_entry_id().as_deref(),
            Some("mem-delete-1")
        );

        let empty = OpsShellControlsQuery::default();
        assert_eq!(empty.requested_memory_deleted_entry_id(), None);
    }

    #[test]
    fn unit_requested_memory_detail_entry_id_trims_and_normalizes_empty_values() {
        let valid = OpsShellControlsQuery {
            detail_memory_id: " mem-detail-1 ".to_string(),
            ..OpsShellControlsQuery::default()
        };
        assert_eq!(
            valid.requested_memory_detail_entry_id().as_deref(),
            Some("mem-detail-1")
        );

        let empty = OpsShellControlsQuery::default();
        assert_eq!(empty.requested_memory_detail_entry_id(), None);
    }
}
