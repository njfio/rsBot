//! Authorization policy helpers for RL lifecycle control-plane actions.

use anyhow::{bail, Result};
use std::path::Path;
use tracing::{debug, warn};

/// Supported lifecycle actions for RL control-plane operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RlLifecycleAction {
    Status,
    Pause,
    Resume,
    Cancel,
    Rollback,
}

impl RlLifecycleAction {
    /// Stable snake_case action label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Cancel => "cancel",
            Self::Rollback => "rollback",
        }
    }
}

/// Parses a lifecycle action token.
pub fn parse_rl_lifecycle_action(raw: &str) -> Result<RlLifecycleAction> {
    let token = raw.trim().to_ascii_lowercase();
    let action = match token.as_str() {
        "status" => RlLifecycleAction::Status,
        "pause" => RlLifecycleAction::Pause,
        "resume" => RlLifecycleAction::Resume,
        "cancel" => RlLifecycleAction::Cancel,
        "rollback" => RlLifecycleAction::Rollback,
        _ => {
            bail!(
                "unsupported rl lifecycle action '{}'; expected status|pause|resume|cancel|rollback",
                raw
            )
        }
    };
    debug!(raw, action = action.as_str(), "parsed rl lifecycle action");
    Ok(action)
}

/// Maps lifecycle actions to RBAC action keys.
pub fn rl_lifecycle_action_key(action: RlLifecycleAction) -> &'static str {
    match action {
        RlLifecycleAction::Status => "control:rl:status",
        RlLifecycleAction::Pause => "control:rl:pause",
        RlLifecycleAction::Resume => "control:rl:resume",
        RlLifecycleAction::Cancel => "control:rl:cancel",
        RlLifecycleAction::Rollback => "control:rl:rollback",
    }
}

/// Authorizes one lifecycle action against the default RBAC policy path.
pub fn authorize_rl_lifecycle_action(
    principal: &str,
    action: RlLifecycleAction,
) -> Result<crate::RbacDecision> {
    let action_key = rl_lifecycle_action_key(action);
    let decision = crate::authorize_action_for_principal(principal, action_key)?;
    debug!(
        principal,
        action = action_key,
        allowed = decision.is_allowed(),
        reason_code = decision.reason_code(),
        "authorized rl lifecycle action"
    );
    Ok(decision)
}

/// Authorizes one lifecycle action against an explicit RBAC policy path.
pub fn authorize_rl_lifecycle_action_with_policy_path(
    principal: &str,
    action: RlLifecycleAction,
    policy_path: &Path,
) -> Result<crate::RbacDecision> {
    let action_key = rl_lifecycle_action_key(action);
    let decision =
        crate::authorize_action_for_principal_with_policy_path(principal, action_key, policy_path)?;
    debug!(
        principal,
        action = action_key,
        policy = %policy_path.display(),
        allowed = decision.is_allowed(),
        reason_code = decision.reason_code(),
        "authorized rl lifecycle action with explicit policy path"
    );
    Ok(decision)
}

/// Enforces lifecycle authorization, returning an actionable error when denied.
pub fn enforce_rl_lifecycle_action_with_policy_path(
    principal: &str,
    action: RlLifecycleAction,
    policy_path: &Path,
) -> Result<()> {
    let action_key = rl_lifecycle_action_key(action);
    let decision = authorize_rl_lifecycle_action_with_policy_path(principal, action, policy_path)?;
    if decision.is_allowed() {
        return Ok(());
    }
    warn!(
        principal,
        action = action_key,
        reason_code = decision.reason_code(),
        "denied rl lifecycle action by policy"
    );
    bail!(
        "unauthorized rl lifecycle action: principal={} action={} reason_code={}",
        principal,
        action_key,
        decision.reason_code()
    )
}

#[cfg(test)]
mod tests {
    use super::{
        authorize_rl_lifecycle_action_with_policy_path,
        enforce_rl_lifecycle_action_with_policy_path, parse_rl_lifecycle_action,
        rl_lifecycle_action_key, RlLifecycleAction,
    };
    use std::path::Path;
    use tempfile::tempdir;

    fn write_policy(path: &Path, payload: &serde_json::Value) {
        std::fs::write(path, format!("{payload}\n")).expect("write policy");
    }

    #[test]
    fn unit_parse_rl_lifecycle_action_accepts_supported_tokens() {
        assert_eq!(
            parse_rl_lifecycle_action("status").expect("status token"),
            RlLifecycleAction::Status
        );
        assert_eq!(
            parse_rl_lifecycle_action("pause").expect("pause token"),
            RlLifecycleAction::Pause
        );
        assert_eq!(
            parse_rl_lifecycle_action("resume").expect("resume token"),
            RlLifecycleAction::Resume
        );
        assert_eq!(
            parse_rl_lifecycle_action("cancel").expect("cancel token"),
            RlLifecycleAction::Cancel
        );
        assert_eq!(
            parse_rl_lifecycle_action("rollback").expect("rollback token"),
            RlLifecycleAction::Rollback
        );
    }

    #[test]
    fn unit_parse_rl_lifecycle_action_rejects_unsupported_tokens() {
        let error = parse_rl_lifecycle_action("promote").expect_err("token should be rejected");
        let message = error.to_string();
        assert!(message.contains("unsupported rl lifecycle action"));
        assert!(message.contains("status|pause|resume|cancel|rollback"));
    }

    #[test]
    fn functional_authorize_rl_lifecycle_action_honors_policy_permissions() {
        let temp = tempdir().expect("tempdir");
        let policy_path = temp.path().join("rbac.json");
        write_policy(
            &policy_path,
            &serde_json::json!({
                "schema_version": 1,
                "team_mode": true,
                "bindings": [
                    { "principal": "local:rl-operator", "roles": ["rl-control"] },
                    { "principal": "local:rl-viewer", "roles": ["rl-view"] }
                ],
                "roles": {
                    "rl-control": {
                        "allow": ["control:rl:*"]
                    },
                    "rl-view": {
                        "allow": ["control:rl:status"]
                    }
                }
            }),
        );

        let operator_pause = authorize_rl_lifecycle_action_with_policy_path(
            "local:rl-operator",
            RlLifecycleAction::Pause,
            &policy_path,
        )
        .expect("operator pause");
        assert!(operator_pause.is_allowed());

        let viewer_status = authorize_rl_lifecycle_action_with_policy_path(
            "local:rl-viewer",
            RlLifecycleAction::Status,
            &policy_path,
        )
        .expect("viewer status");
        assert!(viewer_status.is_allowed());

        let viewer_pause = authorize_rl_lifecycle_action_with_policy_path(
            "local:rl-viewer",
            RlLifecycleAction::Pause,
            &policy_path,
        )
        .expect("viewer pause");
        assert!(!viewer_pause.is_allowed());
        assert_eq!(viewer_pause.reason_code(), "deny_no_matching_allow");
    }

    #[test]
    fn regression_enforce_rl_lifecycle_action_blocks_unauthorized_principal() {
        let temp = tempdir().expect("tempdir");
        let policy_path = temp.path().join("rbac.json");
        write_policy(
            &policy_path,
            &serde_json::json!({
                "schema_version": 1,
                "team_mode": true,
                "bindings": [
                    { "principal": "local:rl-viewer", "roles": ["rl-view"] }
                ],
                "roles": {
                    "rl-view": {
                        "allow": ["control:rl:status"]
                    }
                }
            }),
        );

        let error = enforce_rl_lifecycle_action_with_policy_path(
            "local:rl-viewer",
            RlLifecycleAction::Pause,
            &policy_path,
        )
        .expect_err("viewer pause should be denied");
        let message = error.to_string();
        assert!(message.contains("unauthorized rl lifecycle action"));
        assert!(message.contains("principal=local:rl-viewer"));
        assert!(message.contains("action=control:rl:pause"));
    }

    #[test]
    fn regression_rl_lifecycle_action_key_is_stable() {
        assert_eq!(
            rl_lifecycle_action_key(RlLifecycleAction::Status),
            "control:rl:status"
        );
        assert_eq!(
            rl_lifecycle_action_key(RlLifecycleAction::Pause),
            "control:rl:pause"
        );
        assert_eq!(
            rl_lifecycle_action_key(RlLifecycleAction::Resume),
            "control:rl:resume"
        );
        assert_eq!(
            rl_lifecycle_action_key(RlLifecycleAction::Cancel),
            "control:rl:cancel"
        );
        assert_eq!(
            rl_lifecycle_action_key(RlLifecycleAction::Rollback),
            "control:rl:rollback"
        );
    }
}
