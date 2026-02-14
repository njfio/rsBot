use std::collections::HashSet;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const CUSTOM_COMMAND_POLICY_SCHEMA_VERSION: u32 = 1;
pub const CUSTOM_COMMAND_SANDBOX_RESTRICTED: &str = "restricted";
pub const CUSTOM_COMMAND_SANDBOX_WORKSPACE_WRITE: &str = "workspace_write";
pub const CUSTOM_COMMAND_SANDBOX_UNRESTRICTED: &str = "unrestricted";

fn custom_command_policy_schema_version() -> u32 {
    CUSTOM_COMMAND_POLICY_SCHEMA_VERSION
}

fn default_custom_command_policy_require_approval() -> bool {
    true
}

fn default_custom_command_policy_allow_shell() -> bool {
    false
}

fn default_custom_command_policy_allow_network() -> bool {
    false
}

fn default_custom_command_policy_sandbox_profile() -> String {
    CUSTOM_COMMAND_SANDBOX_RESTRICTED.to_string()
}

fn default_custom_command_policy_denied_env() -> Vec<String> {
    vec![
        "aws_secret_access_key".to_string(),
        "gcp_service_account_key".to_string(),
        "openai_api_key".to_string(),
        "anthropic_api_key".to_string(),
        "google_api_key".to_string(),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `CustomCommandExecutionPolicy` used across Tau components.
pub struct CustomCommandExecutionPolicy {
    #[serde(default = "custom_command_policy_schema_version")]
    pub schema_version: u32,
    #[serde(default = "default_custom_command_policy_require_approval")]
    pub require_approval: bool,
    #[serde(default = "default_custom_command_policy_allow_shell")]
    pub allow_shell: bool,
    #[serde(default = "default_custom_command_policy_allow_network")]
    pub allow_network: bool,
    #[serde(default = "default_custom_command_policy_sandbox_profile")]
    pub sandbox_profile: String,
    #[serde(default)]
    pub allowed_env: Vec<String>,
    #[serde(default = "default_custom_command_policy_denied_env")]
    pub denied_env: Vec<String>,
}

impl Default for CustomCommandExecutionPolicy {
    fn default() -> Self {
        Self {
            schema_version: CUSTOM_COMMAND_POLICY_SCHEMA_VERSION,
            require_approval: default_custom_command_policy_require_approval(),
            allow_shell: default_custom_command_policy_allow_shell(),
            allow_network: default_custom_command_policy_allow_network(),
            sandbox_profile: default_custom_command_policy_sandbox_profile(),
            allowed_env: Vec::new(),
            denied_env: default_custom_command_policy_denied_env(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `CustomCommandSpec` used across Tau components.
pub struct CustomCommandSpec {
    #[serde(default = "custom_command_policy_schema_version")]
    pub schema_version: u32,
    pub name: String,
    pub template: String,
    #[serde(default)]
    pub default_arguments: Value,
    #[serde(default)]
    pub execution_policy: CustomCommandExecutionPolicy,
}

pub fn default_custom_command_execution_policy() -> CustomCommandExecutionPolicy {
    CustomCommandExecutionPolicy::default()
}

pub fn supported_custom_command_sandbox_profiles() -> &'static [&'static str] {
    &[
        CUSTOM_COMMAND_SANDBOX_RESTRICTED,
        CUSTOM_COMMAND_SANDBOX_WORKSPACE_WRITE,
        CUSTOM_COMMAND_SANDBOX_UNRESTRICTED,
    ]
}

pub fn validate_custom_command_execution_policy(
    policy: &CustomCommandExecutionPolicy,
) -> Result<()> {
    if policy.schema_version != CUSTOM_COMMAND_POLICY_SCHEMA_VERSION {
        bail!(
            "unsupported custom command policy schema version {} (expected {})",
            policy.schema_version,
            CUSTOM_COMMAND_POLICY_SCHEMA_VERSION
        );
    }
    let profile = policy.sandbox_profile.trim().to_ascii_lowercase();
    if !supported_custom_command_sandbox_profiles().contains(&profile.as_str()) {
        bail!(
            "unsupported custom command sandbox profile '{}' (supported: restricted, workspace_write, unrestricted)",
            policy.sandbox_profile
        );
    }

    let mut allowed_seen = HashSet::new();
    for key in &policy.allowed_env {
        if !is_valid_env_key(key) {
            bail!(
                "custom command policy allowed_env contains invalid key '{}'",
                key
            );
        }
        let normalized = normalize_env_key(key);
        if !allowed_seen.insert(normalized.clone()) {
            bail!(
                "custom command policy allowed_env contains duplicate key '{}'",
                key
            );
        }
    }

    let mut denied_seen = HashSet::new();
    for key in &policy.denied_env {
        if !is_valid_env_key(key) {
            bail!(
                "custom command policy denied_env contains invalid key '{}'",
                key
            );
        }
        let normalized = normalize_env_key(key);
        if !denied_seen.insert(normalized.clone()) {
            bail!(
                "custom command policy denied_env contains duplicate key '{}'",
                key
            );
        }
    }

    for key in &allowed_seen {
        if denied_seen.contains(key) {
            bail!(
                "custom command policy key '{}' appears in both allowed_env and denied_env",
                key
            );
        }
    }
    Ok(())
}

pub fn validate_custom_command_spec(spec: &CustomCommandSpec) -> Result<()> {
    if spec.schema_version != CUSTOM_COMMAND_POLICY_SCHEMA_VERSION {
        bail!(
            "unsupported custom command spec schema version {} (expected {})",
            spec.schema_version,
            CUSTOM_COMMAND_POLICY_SCHEMA_VERSION
        );
    }
    let name = spec.name.trim();
    if !is_valid_command_name(name) {
        bail!("custom command spec has invalid name '{}'", spec.name);
    }
    if spec.template.trim().is_empty() {
        bail!("custom command spec '{}' has empty template", spec.name);
    }
    validate_custom_command_execution_policy(&spec.execution_policy)?;
    validate_custom_command_template_and_arguments(
        spec.template.as_str(),
        &spec.default_arguments,
        &spec.execution_policy,
    )?;
    Ok(())
}

pub fn validate_custom_command_template_and_arguments(
    template: &str,
    arguments: &Value,
    policy: &CustomCommandExecutionPolicy,
) -> Result<()> {
    validate_custom_command_execution_policy(policy)?;

    if !arguments.is_object() {
        bail!("custom command arguments must be a JSON object");
    }
    if template.trim().is_empty() {
        bail!("custom command template cannot be empty");
    }

    if !policy.allow_shell && template_contains_shell_control_operators(template) {
        bail!("custom command template contains shell control operators while allow_shell=false");
    }

    let placeholders = extract_template_placeholders(template)?;
    let allowed_env: HashSet<String> = policy
        .allowed_env
        .iter()
        .map(|value| normalize_env_key(value))
        .collect();
    let denied_env: HashSet<String> = policy
        .denied_env
        .iter()
        .map(|value| normalize_env_key(value))
        .collect();

    for placeholder in placeholders {
        if !is_valid_env_key(placeholder.as_str()) {
            bail!(
                "custom command template contains invalid placeholder '{{{{{}}}}}'",
                placeholder
            );
        }
        let normalized = normalize_env_key(placeholder.as_str());
        if denied_env.contains(&normalized) {
            bail!(
                "custom command template placeholder '{}' is denied by policy",
                placeholder
            );
        }
        if !allowed_env.is_empty() && !allowed_env.contains(&normalized) {
            bail!(
                "custom command template placeholder '{}' is not allowlisted by policy",
                placeholder
            );
        }
    }

    if let Some(map) = arguments.as_object() {
        for key in map.keys() {
            if !is_valid_env_key(key) {
                bail!("custom command arguments include invalid key '{}'", key);
            }
        }
    }

    Ok(())
}

pub fn normalize_sandbox_profile(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        CUSTOM_COMMAND_SANDBOX_WORKSPACE_WRITE => {
            CUSTOM_COMMAND_SANDBOX_WORKSPACE_WRITE.to_string()
        }
        CUSTOM_COMMAND_SANDBOX_UNRESTRICTED => CUSTOM_COMMAND_SANDBOX_UNRESTRICTED.to_string(),
        _ => CUSTOM_COMMAND_SANDBOX_RESTRICTED.to_string(),
    }
}

pub fn is_valid_env_key(raw: &str) -> bool {
    if raw.trim().is_empty() {
        return false;
    }
    let mut chars = raw.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

pub fn normalize_env_key(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

pub fn is_valid_command_name(raw: &str) -> bool {
    if raw.is_empty() {
        return false;
    }
    let mut chars = raw.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

fn template_contains_shell_control_operators(template: &str) -> bool {
    const SHELL_DENYLIST: [&str; 8] = ["&&", "||", ";", "`", "$(", "|", "\n", "\r"];
    SHELL_DENYLIST.iter().any(|token| template.contains(token))
}

fn extract_template_placeholders(template: &str) -> Result<Vec<String>> {
    let mut vars = Vec::new();
    let mut start_index = 0usize;
    while let Some(open_rel) = template[start_index..].find("{{") {
        let open = start_index.saturating_add(open_rel);
        let close_search_start = open.saturating_add(2);
        let Some(close_rel) = template[close_search_start..].find("}}") else {
            bail!("custom command template has unterminated '{{{{' placeholder");
        };
        let close = close_search_start.saturating_add(close_rel);
        let raw = template[close_search_start..close].trim();
        if raw.is_empty() {
            bail!("custom command template contains empty '{{{{}}}}' placeholder");
        }
        vars.push(raw.to_string());
        start_index = close.saturating_add(2);
    }
    Ok(vars)
}

#[cfg(test)]
mod tests {
    use super::{
        default_custom_command_execution_policy, normalize_sandbox_profile,
        validate_custom_command_execution_policy, validate_custom_command_spec,
        validate_custom_command_template_and_arguments, CustomCommandExecutionPolicy,
        CustomCommandSpec, CUSTOM_COMMAND_SANDBOX_RESTRICTED,
    };
    use serde_json::json;

    #[test]
    fn unit_default_policy_is_deny_by_default_for_shell_and_network() {
        let policy = default_custom_command_execution_policy();
        assert!(policy.require_approval);
        assert!(!policy.allow_shell);
        assert!(!policy.allow_network);
        assert_eq!(policy.sandbox_profile, CUSTOM_COMMAND_SANDBOX_RESTRICTED);
        assert!(!policy.denied_env.is_empty());
    }

    #[test]
    fn unit_validate_policy_rejects_allow_and_deny_overlap() {
        let policy = CustomCommandExecutionPolicy {
            allowed_env: vec!["DEPLOY_ENV".to_string()],
            denied_env: vec!["deploy_env".to_string()],
            ..CustomCommandExecutionPolicy::default()
        };
        let error =
            validate_custom_command_execution_policy(&policy).expect_err("should reject overlap");
        assert!(error
            .to_string()
            .contains("appears in both allowed_env and denied_env"));
    }

    #[test]
    fn functional_validate_template_and_arguments_accepts_allowlisted_placeholders() {
        let policy = CustomCommandExecutionPolicy {
            allowed_env: vec!["DEPLOY_ENV".to_string(), "REGION".to_string()],
            denied_env: vec!["AWS_SECRET_ACCESS_KEY".to_string()],
            ..CustomCommandExecutionPolicy::default()
        };
        validate_custom_command_template_and_arguments(
            "deploy {{deploy_env}} --region {{region}}",
            &json!({"deploy_env":"staging","region":"us-west-2"}),
            &policy,
        )
        .expect("allowlisted placeholders should pass");
    }

    #[test]
    fn functional_validate_spec_accepts_policy_and_template_contract() {
        let spec = CustomCommandSpec {
            schema_version: 1,
            name: "deploy_release".to_string(),
            template: "deploy {{deploy_env}}".to_string(),
            default_arguments: json!({"deploy_env":"staging"}),
            execution_policy: CustomCommandExecutionPolicy {
                allowed_env: vec!["DEPLOY_ENV".to_string()],
                denied_env: vec!["OPENAI_API_KEY".to_string()],
                ..CustomCommandExecutionPolicy::default()
            },
        };
        validate_custom_command_spec(&spec).expect("spec should validate");
    }

    #[test]
    fn regression_validate_template_rejects_shell_operators_when_disallowed() {
        let policy = default_custom_command_execution_policy();
        let error = validate_custom_command_template_and_arguments(
            "deploy {{env}} && curl https://example.com",
            &json!({"env":"prod"}),
            &policy,
        )
        .expect_err("shell operators should be rejected");
        assert!(error
            .to_string()
            .contains("shell control operators while allow_shell=false"));
    }

    #[test]
    fn regression_validate_policy_rejects_invalid_sandbox_profile() {
        let policy = CustomCommandExecutionPolicy {
            sandbox_profile: "invalid-profile".to_string(),
            ..CustomCommandExecutionPolicy::default()
        };
        let error =
            validate_custom_command_execution_policy(&policy).expect_err("sandbox should fail");
        assert!(error
            .to_string()
            .contains("unsupported custom command sandbox profile"));
    }

    #[test]
    fn regression_normalize_sandbox_profile_falls_back_to_restricted() {
        assert_eq!(
            normalize_sandbox_profile("unknown"),
            CUSTOM_COMMAND_SANDBOX_RESTRICTED
        );
    }
}
