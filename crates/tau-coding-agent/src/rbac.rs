use super::*;

use std::collections::{BTreeMap, BTreeSet};

pub(crate) const RBAC_USAGE: &str = "usage: /rbac <check|whoami> ...";

const RBAC_SCHEMA_VERSION: u32 = 1;
const RBAC_POLICY_PATH_ENV: &str = "TAU_RBAC_POLICY_PATH";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
struct RbacRolePolicy {
    #[serde(default)]
    allow: Vec<String>,
    #[serde(default)]
    deny: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
struct RbacBinding {
    principal: String,
    #[serde(default)]
    roles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
struct RbacPolicyFile {
    #[serde(default = "rbac_schema_version")]
    schema_version: u32,
    #[serde(default)]
    team_mode: bool,
    #[serde(default)]
    bindings: Vec<RbacBinding>,
    #[serde(default)]
    roles: BTreeMap<String, RbacRolePolicy>,
}

impl Default for RbacPolicyFile {
    fn default() -> Self {
        Self {
            schema_version: RBAC_SCHEMA_VERSION,
            team_mode: false,
            bindings: Vec::new(),
            roles: BTreeMap::new(),
        }
    }
}

fn rbac_schema_version() -> u32 {
    RBAC_SCHEMA_VERSION
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub(crate) enum RbacDecision {
    Allow {
        reason_code: String,
        matched_role: Option<String>,
        matched_pattern: Option<String>,
    },
    Deny {
        reason_code: String,
        matched_role: Option<String>,
        matched_pattern: Option<String>,
    },
}

impl RbacDecision {
    pub(crate) fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow { .. })
    }

    pub(crate) fn reason_code(&self) -> &str {
        match self {
            Self::Allow { reason_code, .. } | Self::Deny { reason_code, .. } => reason_code,
        }
    }

    pub(crate) fn matched_role(&self) -> Option<&str> {
        match self {
            Self::Allow { matched_role, .. } | Self::Deny { matched_role, .. } => {
                matched_role.as_deref()
            }
        }
    }

    pub(crate) fn matched_pattern(&self) -> Option<&str> {
        match self {
            Self::Allow {
                matched_pattern, ..
            }
            | Self::Deny {
                matched_pattern, ..
            } => matched_pattern.as_deref(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RbacOutputFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RbacCommand {
    WhoAmI {
        principal: String,
        format: RbacOutputFormat,
    },
    Check {
        principal: String,
        action: String,
        format: RbacOutputFormat,
    },
}

pub(crate) fn resolve_local_principal() -> String {
    local_principal(None)
}

pub(crate) fn local_principal(actor_override: Option<&str>) -> String {
    let actor = actor_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| std::env::var("TAU_RBAC_LOCAL_ACTOR").ok())
        .or_else(|| std::env::var("USER").ok())
        .or_else(|| std::env::var("LOGNAME").ok())
        .unwrap_or_else(|| "operator".to_string());
    format!("local:{}", sanitize_principal_component(&actor))
}

pub(crate) fn github_principal(login: &str) -> String {
    format!("github:{}", sanitize_principal_component(login))
}

pub(crate) fn slack_principal(user_id: &str) -> String {
    format!("slack:{}", sanitize_principal_component(user_id))
}

pub(crate) fn discord_principal(user_id: &str) -> String {
    format!("discord:{}", sanitize_principal_component(user_id))
}

pub(crate) fn telegram_principal(user_id: &str) -> String {
    format!("telegram:{}", sanitize_principal_component(user_id))
}

pub(crate) fn authorize_command_for_principal(
    principal: &str,
    command_name: &str,
) -> Result<RbacDecision> {
    authorize_action_for_principal(principal, format!("command:{command_name}").as_str())
}

pub(crate) fn authorize_tool_for_principal(
    principal: Option<&str>,
    tool_name: &str,
) -> Result<RbacDecision> {
    let policy_path = default_rbac_policy_path();
    authorize_tool_for_principal_with_policy_path(principal, tool_name, &policy_path)
}

pub(crate) fn authorize_tool_for_principal_with_policy_path(
    principal: Option<&str>,
    tool_name: &str,
    policy_path: &Path,
) -> Result<RbacDecision> {
    let principal = principal
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(resolve_local_principal);
    authorize_action_for_principal_with_policy_path(
        &principal,
        format!("tool:{tool_name}").as_str(),
        policy_path,
    )
}

pub(crate) fn authorize_action_for_principal(
    principal: &str,
    action: &str,
) -> Result<RbacDecision> {
    let policy_path = default_rbac_policy_path();
    authorize_action_for_principal_with_policy_path(principal, action, &policy_path)
}

pub(crate) fn execute_rbac_command(command_args: &str) -> String {
    let policy_path = default_rbac_policy_path();
    execute_rbac_command_with_path(command_args, &policy_path)
}

pub(crate) fn authorize_action_for_principal_with_policy_path(
    principal: &str,
    action: &str,
    policy_path: &Path,
) -> Result<RbacDecision> {
    let policy = load_rbac_policy(policy_path)?;
    Ok(evaluate_policy(policy, principal, action))
}

fn execute_rbac_command_with_path(command_args: &str, policy_path: &Path) -> String {
    let command = match parse_rbac_command(command_args) {
        Ok(command) => command,
        Err(_) => return RBAC_USAGE.to_string(),
    };

    match command {
        RbacCommand::WhoAmI { principal, format } => {
            match render_whoami(principal.as_str(), format, policy_path) {
                Ok(output) => output,
                Err(error) => format!("rbac error: {error}"),
            }
        }
        RbacCommand::Check {
            principal,
            action,
            format,
        } => match render_check(principal.as_str(), action.as_str(), format, policy_path) {
            Ok(output) => output,
            Err(error) => format!("rbac error: {error}"),
        },
    }
}

fn render_whoami(principal: &str, format: RbacOutputFormat, policy_path: &Path) -> Result<String> {
    let policy = load_rbac_policy(policy_path)?;
    let roles = resolve_roles_for_principal(&policy, principal);
    let roles_text = if roles.is_empty() {
        "none".to_string()
    } else {
        roles.join(",")
    };

    Ok(match format {
        RbacOutputFormat::Text => format!(
            "rbac whoami: principal={} team_mode={} roles={} policy={}",
            principal,
            policy.team_mode,
            roles_text,
            policy_path.display()
        ),
        RbacOutputFormat::Json => serde_json::json!({
            "principal": principal,
            "team_mode": policy.team_mode,
            "roles": roles,
            "policy_path": policy_path.display().to_string(),
        })
        .to_string(),
    })
}

fn render_check(
    principal: &str,
    action: &str,
    format: RbacOutputFormat,
    policy_path: &Path,
) -> Result<String> {
    let decision = authorize_action_for_principal_with_policy_path(principal, action, policy_path)?;
    let decision_label = if decision.is_allowed() {
        "allow"
    } else {
        "deny"
    };
    let matched_role = decision.matched_role().unwrap_or("none");
    let matched_pattern = decision.matched_pattern().unwrap_or("none");

    Ok(match format {
        RbacOutputFormat::Text => format!(
            "rbac check: principal={} action={} decision={} reason_code={} matched_role={} matched_pattern={} policy={}",
            principal,
            action,
            decision_label,
            decision.reason_code(),
            matched_role,
            matched_pattern,
            policy_path.display()
        ),
        RbacOutputFormat::Json => serde_json::json!({
            "principal": principal,
            "action": action,
            "decision": decision,
            "policy_path": policy_path.display().to_string(),
        })
        .to_string(),
    })
}

fn parse_rbac_command(command_args: &str) -> Result<RbacCommand> {
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        bail!("{RBAC_USAGE}");
    }

    match tokens[0] {
        "whoami" => parse_rbac_whoami(&tokens[1..]),
        "check" => parse_rbac_check(&tokens[1..]),
        _ => bail!("{RBAC_USAGE}"),
    }
}

fn parse_rbac_whoami(tokens: &[&str]) -> Result<RbacCommand> {
    let mut format = RbacOutputFormat::Text;
    let mut principal = None;
    let mut channel = None;
    let mut actor = None;

    let mut index = 0usize;
    while index < tokens.len() {
        match tokens[index] {
            "--json" => {
                format = RbacOutputFormat::Json;
                index += 1;
            }
            "--principal" => {
                index += 1;
                let Some(value) = tokens.get(index) else {
                    bail!("{RBAC_USAGE}");
                };
                principal = Some((*value).to_string());
                index += 1;
            }
            "--channel" => {
                index += 1;
                let Some(value) = tokens.get(index) else {
                    bail!("{RBAC_USAGE}");
                };
                channel = Some((*value).to_string());
                index += 1;
            }
            "--actor" => {
                index += 1;
                let Some(value) = tokens.get(index) else {
                    bail!("{RBAC_USAGE}");
                };
                actor = Some((*value).to_string());
                index += 1;
            }
            _ => bail!("{RBAC_USAGE}"),
        }
    }

    let principal = resolve_principal_input(principal, channel, actor)?;
    Ok(RbacCommand::WhoAmI { principal, format })
}

fn parse_rbac_check(tokens: &[&str]) -> Result<RbacCommand> {
    let Some(action) = tokens.first() else {
        bail!("{RBAC_USAGE}");
    };
    if action.starts_with("--") {
        bail!("{RBAC_USAGE}");
    }
    let mut format = RbacOutputFormat::Text;
    let mut principal = None;
    let mut channel = None;
    let mut actor = None;
    let mut index = 1usize;
    while index < tokens.len() {
        match tokens[index] {
            "--json" => {
                format = RbacOutputFormat::Json;
                index += 1;
            }
            "--principal" => {
                index += 1;
                let Some(value) = tokens.get(index) else {
                    bail!("{RBAC_USAGE}");
                };
                principal = Some((*value).to_string());
                index += 1;
            }
            "--channel" => {
                index += 1;
                let Some(value) = tokens.get(index) else {
                    bail!("{RBAC_USAGE}");
                };
                channel = Some((*value).to_string());
                index += 1;
            }
            "--actor" => {
                index += 1;
                let Some(value) = tokens.get(index) else {
                    bail!("{RBAC_USAGE}");
                };
                actor = Some((*value).to_string());
                index += 1;
            }
            _ => bail!("{RBAC_USAGE}"),
        }
    }

    let principal = resolve_principal_input(principal, channel, actor)?;
    Ok(RbacCommand::Check {
        principal,
        action: (*action).to_string(),
        format,
    })
}

fn resolve_principal_input(
    principal: Option<String>,
    channel: Option<String>,
    actor: Option<String>,
) -> Result<String> {
    if let Some(principal) = principal {
        let principal = principal.trim();
        if principal.is_empty() {
            bail!("rbac principal must not be empty");
        }
        return Ok(principal.to_string());
    }

    let channel = channel.unwrap_or_else(|| "local".to_string());
    match channel.as_str() {
        "local" => Ok(local_principal(actor.as_deref())),
        "github" => {
            let actor = actor
                .as_deref()
                .ok_or_else(|| anyhow!("rbac --channel github requires --actor"))?;
            Ok(github_principal(actor))
        }
        "slack" => {
            let actor = actor
                .as_deref()
                .ok_or_else(|| anyhow!("rbac --channel slack requires --actor"))?;
            Ok(slack_principal(actor))
        }
        "discord" => {
            let actor = actor
                .as_deref()
                .ok_or_else(|| anyhow!("rbac --channel discord requires --actor"))?;
            Ok(discord_principal(actor))
        }
        "telegram" => {
            let actor = actor
                .as_deref()
                .ok_or_else(|| anyhow!("rbac --channel telegram requires --actor"))?;
            Ok(telegram_principal(actor))
        }
        _ => bail!(
            "unsupported rbac channel '{}': expected local|github|slack|discord|telegram",
            channel
        ),
    }
}

fn default_rbac_policy_path() -> PathBuf {
    std::env::var(RBAC_POLICY_PATH_ENV)
        .ok()
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".tau/security/rbac.json"))
}

pub(crate) fn rbac_policy_path_for_state_dir(state_dir: &Path) -> PathBuf {
    let state_name = state_dir.file_name().and_then(|value| value.to_str());
    let tau_root = match state_name {
        Some("github")
        | Some("github-issues")
        | Some("slack")
        | Some("discord")
        | Some("telegram")
        | Some("events")
        | Some("channel-store") => state_dir
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(state_dir),
        _ => state_dir,
    };
    tau_root.join("security/rbac.json")
}

fn load_rbac_policy(path: &Path) -> Result<RbacPolicyFile> {
    if !path.exists() {
        return Ok(RbacPolicyFile::default());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read rbac policy file {}", path.display()))?;
    let parsed = serde_json::from_str::<RbacPolicyFile>(&raw)
        .with_context(|| format!("failed to parse rbac policy file {}", path.display()))?;
    validate_rbac_policy(&parsed)?;
    Ok(parsed)
}

fn validate_rbac_policy(policy: &RbacPolicyFile) -> Result<()> {
    if policy.schema_version != RBAC_SCHEMA_VERSION {
        bail!(
            "unsupported rbac schema version {} (expected {})",
            policy.schema_version,
            RBAC_SCHEMA_VERSION
        );
    }

    let mut seen_principals = BTreeSet::new();
    for binding in &policy.bindings {
        if binding.principal.trim().is_empty() {
            bail!("rbac binding principal must not be empty");
        }
        if !seen_principals.insert(binding.principal.clone()) {
            bail!(
                "duplicate rbac principal binding '{}'; merge roles into one binding",
                binding.principal
            );
        }
        if binding.roles.is_empty() {
            bail!(
                "rbac binding for principal '{}' must include at least one role",
                binding.principal
            );
        }
        for role in &binding.roles {
            if role.trim().is_empty() {
                bail!(
                    "rbac binding for principal '{}' contains empty role id",
                    binding.principal
                );
            }
        }
    }

    for (role_id, role_policy) in &policy.roles {
        if role_id.trim().is_empty() {
            bail!("rbac role id must not be empty");
        }
        if role_policy.allow.is_empty() && role_policy.deny.is_empty() {
            bail!(
                "rbac role '{}' must include at least one allow or deny rule",
                role_id
            );
        }
    }

    Ok(())
}

fn evaluate_policy(policy: RbacPolicyFile, principal: &str, action: &str) -> RbacDecision {
    let principal = principal.trim();
    if !policy.team_mode {
        return RbacDecision::Allow {
            reason_code: "allow_team_mode_disabled".to_string(),
            matched_role: None,
            matched_pattern: None,
        };
    }
    if principal.is_empty() {
        return RbacDecision::Deny {
            reason_code: "deny_principal_missing".to_string(),
            matched_role: None,
            matched_pattern: None,
        };
    }
    let roles = resolve_roles_for_principal(&policy, principal);
    if roles.is_empty() {
        return RbacDecision::Deny {
            reason_code: "deny_unbound_principal".to_string(),
            matched_role: None,
            matched_pattern: None,
        };
    }

    for role in &roles {
        let Some(role_policy) = policy.roles.get(role) else {
            continue;
        };
        for pattern in &role_policy.deny {
            if permission_pattern_matches(pattern, action) {
                return RbacDecision::Deny {
                    reason_code: "deny_role_policy".to_string(),
                    matched_role: Some(role.clone()),
                    matched_pattern: Some(pattern.clone()),
                };
            }
        }
    }

    for role in &roles {
        let Some(role_policy) = policy.roles.get(role) else {
            continue;
        };
        for pattern in &role_policy.allow {
            if permission_pattern_matches(pattern, action) {
                return RbacDecision::Allow {
                    reason_code: "allow_role_policy".to_string(),
                    matched_role: Some(role.clone()),
                    matched_pattern: Some(pattern.clone()),
                };
            }
        }
    }

    RbacDecision::Deny {
        reason_code: "deny_no_matching_allow".to_string(),
        matched_role: None,
        matched_pattern: None,
    }
}

fn resolve_roles_for_principal(policy: &RbacPolicyFile, principal: &str) -> Vec<String> {
    let mut roles = BTreeSet::new();
    for binding in &policy.bindings {
        if principal_pattern_matches(binding.principal.as_str(), principal) {
            for role in &binding.roles {
                let role = role.trim();
                if !role.is_empty() {
                    roles.insert(role.to_string());
                }
            }
        }
    }
    roles.into_iter().collect::<Vec<_>>()
}

fn principal_pattern_matches(pattern: &str, principal: &str) -> bool {
    wildcard_pattern_matches(pattern, principal)
}

fn permission_pattern_matches(pattern: &str, action: &str) -> bool {
    wildcard_pattern_matches(pattern, action)
}

fn wildcard_pattern_matches(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return value.starts_with(prefix);
    }
    pattern == value
}

fn sanitize_principal_component(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "unknown".to_string();
    }
    let mut output = String::with_capacity(trimmed.len());
    for character in trimmed.chars() {
        let normalized =
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            };
        output.push(normalized.to_ascii_lowercase());
    }
    if output.is_empty() {
        "unknown".to_string()
    } else {
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_policy(path: &Path, payload: &Value) {
        std::fs::write(path, format!("{payload}\n")).expect("write policy");
    }

    #[test]
    fn unit_permission_pattern_matches_supports_exact_prefix_and_wildcard() {
        assert!(permission_pattern_matches("tool:*", "tool:bash"));
        assert!(permission_pattern_matches(
            "command:/session*",
            "command:/session-stats"
        ));
        assert!(permission_pattern_matches("*", "anything"));
        assert!(!permission_pattern_matches("tool:write", "tool:edit"));
    }

    #[test]
    fn functional_execute_rbac_command_reports_whoami_and_check_results() {
        let temp = tempdir().expect("tempdir");
        let policy_path = temp.path().join("rbac.json");
        write_policy(
            &policy_path,
            &serde_json::json!({
                "schema_version": 1,
                "team_mode": true,
                "bindings": [
                    {
                        "principal": "local:alice",
                        "roles": ["owner"]
                    }
                ],
                "roles": {
                    "owner": {
                        "allow": ["command:/policy"],
                        "deny": ["command:/danger"]
                    }
                }
            }),
        );

        let whoami =
            execute_rbac_command_with_path("whoami --principal local:alice", policy_path.as_path());
        assert!(whoami.contains("rbac whoami: principal=local:alice"));
        assert!(whoami.contains("roles=owner"));

        let allowed = execute_rbac_command_with_path(
            "check command:/policy --principal local:alice",
            policy_path.as_path(),
        );
        assert!(allowed.contains("decision=allow"));
        assert!(allowed.contains("reason_code=allow_role_policy"));

        let denied = execute_rbac_command_with_path(
            "check command:/danger --principal local:alice",
            policy_path.as_path(),
        );
        assert!(denied.contains("decision=deny"));
        assert!(denied.contains("reason_code=deny_role_policy"));
    }

    #[test]
    fn integration_authorize_action_supports_channel_specific_principals() {
        let temp = tempdir().expect("tempdir");
        let policy_path = temp.path().join("rbac.json");
        write_policy(
            &policy_path,
            &serde_json::json!({
                "schema_version": 1,
                "team_mode": true,
                "bindings": [
                    {
                        "principal": "github:*",
                        "roles": ["remote-reader"]
                    },
                    {
                        "principal": "slack:ux-team-*",
                        "roles": ["remote-writer"]
                    },
                    {
                        "principal": "discord:dev-*",
                        "roles": ["remote-health"]
                    },
                    {
                        "principal": "telegram:chat-*",
                        "roles": ["remote-health"]
                    }
                ],
                "roles": {
                    "remote-reader": {
                        "allow": ["command:/tau-status"]
                    },
                    "remote-writer": {
                        "allow": ["tool:write"]
                    },
                    "remote-health": {
                        "allow": ["command:/tau-health"]
                    }
                }
            }),
        );

        let github_result = authorize_action_for_principal_with_policy_path(
            github_principal("Alice").as_str(),
            "command:/tau-status",
            policy_path.as_path(),
        )
        .expect("github auth");
        assert!(github_result.is_allowed());

        let slack_result = authorize_action_for_principal_with_policy_path(
            slack_principal("UX-TEAM-42").as_str(),
            "tool:write",
            policy_path.as_path(),
        )
        .expect("slack auth");
        assert!(slack_result.is_allowed());

        let discord_result = authorize_action_for_principal_with_policy_path(
            discord_principal("Dev-7").as_str(),
            "command:/tau-health",
            policy_path.as_path(),
        )
        .expect("discord auth");
        assert!(discord_result.is_allowed());

        let telegram_result = authorize_action_for_principal_with_policy_path(
            telegram_principal("chat-99").as_str(),
            "command:/tau-health",
            policy_path.as_path(),
        )
        .expect("telegram auth");
        assert!(telegram_result.is_allowed());
    }

    #[test]
    fn functional_execute_rbac_command_resolves_discord_and_telegram_channels() {
        let temp = tempdir().expect("tempdir");
        let policy_path = temp.path().join("rbac.json");
        write_policy(
            &policy_path,
            &serde_json::json!({
                "schema_version": 1,
                "team_mode": true,
                "bindings": [
                    {
                        "principal": "discord:dev-42",
                        "roles": ["remote-reader"]
                    },
                    {
                        "principal": "telegram:chat-5",
                        "roles": ["remote-reader"]
                    }
                ],
                "roles": {
                    "remote-reader": {
                        "allow": ["command:/tau-status"]
                    }
                }
            }),
        );

        let discord = execute_rbac_command_with_path(
            "whoami --channel discord --actor Dev#42",
            policy_path.as_path(),
        );
        assert!(discord.contains("principal=discord:dev-42"));

        let telegram = execute_rbac_command_with_path(
            "whoami --channel telegram --actor Chat@5",
            policy_path.as_path(),
        );
        assert!(telegram.contains("principal=telegram:chat-5"));
    }

    #[test]
    fn regression_team_mode_disabled_allows_by_default_for_back_compat() {
        let temp = tempdir().expect("tempdir");
        let policy_path = temp.path().join("rbac.json");
        write_policy(
            &policy_path,
            &serde_json::json!({
                "schema_version": 1,
                "team_mode": false,
                "bindings": [],
                "roles": {}
            }),
        );

        let decision = authorize_action_for_principal_with_policy_path(
            "local:unknown",
            "command:/anything",
            policy_path.as_path(),
        )
        .expect("authorize");
        assert_eq!(
            decision,
            RbacDecision::Allow {
                reason_code: "allow_team_mode_disabled".to_string(),
                matched_role: None,
                matched_pattern: None
            }
        );
    }

    #[test]
    fn regression_parse_rbac_command_rejects_invalid_usage_forms() {
        let error = parse_rbac_command("").expect_err("empty fails");
        assert!(error.to_string().contains(RBAC_USAGE));
        let error = parse_rbac_command("check").expect_err("missing action");
        assert!(error.to_string().contains(RBAC_USAGE));
        let error = parse_rbac_command("whoami --channel github").expect_err("missing actor");
        assert!(error.to_string().contains("requires --actor"));
        let error = parse_rbac_command("whoami --channel discord").expect_err("missing actor");
        assert!(error.to_string().contains("requires --actor"));
        let error = parse_rbac_command("whoami --channel matrix --actor user")
            .expect_err("unsupported channel");
        assert!(error
            .to_string()
            .contains("expected local|github|slack|discord|telegram"));
    }

    #[test]
    fn unit_resolve_local_principal_uses_override_when_provided() {
        assert_eq!(local_principal(Some("Alice Smith")), "local:alice-smith");
    }

    #[test]
    fn unit_channel_principal_helpers_normalize_components() {
        assert_eq!(github_principal("Alice Smith"), "github:alice-smith");
        assert_eq!(slack_principal("UX TEAM#42"), "slack:ux-team-42");
        assert_eq!(discord_principal("Dev#42"), "discord:dev-42");
        assert_eq!(telegram_principal("Chat@5"), "telegram:chat-5");
    }

    #[test]
    fn regression_rbac_policy_path_for_state_dir_supports_future_channel_dirs() {
        let temp = tempdir().expect("tempdir");
        let tau_root = temp.path().join(".tau");
        let discord_state = tau_root.join("discord");
        let telegram_state = tau_root.join("telegram");
        assert_eq!(
            rbac_policy_path_for_state_dir(&discord_state),
            tau_root.join("security/rbac.json")
        );
        assert_eq!(
            rbac_policy_path_for_state_dir(&telegram_state),
            tau_root.join("security/rbac.json")
        );
    }
}
