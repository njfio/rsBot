use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

const ROUTE_TABLE_SCHEMA_VERSION: u32 = 1;
const DEFAULT_ROLE_NAME: &str = "default";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MultiAgentRoleProfile {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub prompt_suffix: Option<String>,
    #[serde(default)]
    pub tool_policy_preset: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiAgentRouteTarget {
    pub role: String,
    #[serde(default)]
    pub fallback_roles: Vec<String>,
}

impl Default for MultiAgentRouteTarget {
    fn default() -> Self {
        Self {
            role: DEFAULT_ROLE_NAME.to_string(),
            fallback_roles: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiAgentRouteTable {
    schema_version: u32,
    #[serde(default = "default_role_profiles")]
    pub roles: BTreeMap<String, MultiAgentRoleProfile>,
    #[serde(default)]
    pub planner: MultiAgentRouteTarget,
    #[serde(default)]
    pub delegated: MultiAgentRouteTarget,
    #[serde(default)]
    pub delegated_categories: BTreeMap<String, MultiAgentRouteTarget>,
    #[serde(default)]
    pub review: MultiAgentRouteTarget,
}

impl Default for MultiAgentRouteTable {
    fn default() -> Self {
        Self {
            schema_version: ROUTE_TABLE_SCHEMA_VERSION,
            roles: default_role_profiles(),
            planner: MultiAgentRouteTarget::default(),
            delegated: MultiAgentRouteTarget::default(),
            delegated_categories: BTreeMap::new(),
            review: MultiAgentRouteTarget::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MultiAgentRoutePhase {
    Planner,
    DelegatedStep,
    Review,
}

impl MultiAgentRoutePhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Planner => "planner",
            Self::DelegatedStep => "delegated-step",
            Self::Review => "review",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiAgentRouteSelection {
    pub phase: MultiAgentRoutePhase,
    pub category: Option<String>,
    pub primary_role: String,
    pub fallback_roles: Vec<String>,
    pub attempt_roles: Vec<String>,
}

fn default_role_profiles() -> BTreeMap<String, MultiAgentRoleProfile> {
    let mut roles = BTreeMap::new();
    roles.insert(
        DEFAULT_ROLE_NAME.to_string(),
        MultiAgentRoleProfile::default(),
    );
    roles
}

pub fn load_multi_agent_route_table(path: &Path) -> Result<MultiAgentRouteTable> {
    if !path.exists() {
        return Ok(MultiAgentRouteTable::default());
    }

    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read orchestrator route table {}", path.display()))?;
    parse_multi_agent_route_table_with_source(&raw, &path.display().to_string())
}

pub fn parse_multi_agent_route_table(raw: &str) -> Result<MultiAgentRouteTable> {
    parse_multi_agent_route_table_with_source(raw, "<inline-route-table>")
}

fn parse_multi_agent_route_table_with_source(
    raw: &str,
    source_label: &str,
) -> Result<MultiAgentRouteTable> {
    let mut parsed = serde_json::from_str::<MultiAgentRouteTable>(raw)
        .with_context(|| format!("failed to parse orchestrator route table {}", source_label))?;
    normalize_and_validate_route_table(source_label, &mut parsed)?;
    Ok(parsed)
}

fn normalize_and_validate_route_table(
    source_label: &str,
    table: &mut MultiAgentRouteTable,
) -> Result<()> {
    if table.schema_version != ROUTE_TABLE_SCHEMA_VERSION {
        bail!(
            "unsupported orchestrator route table schema_version {} in {} (expected {})",
            table.schema_version,
            source_label,
            ROUTE_TABLE_SCHEMA_VERSION
        );
    }

    let mut normalized_roles = BTreeMap::new();
    for (raw_role, profile) in std::mem::take(&mut table.roles) {
        let role = normalize_role_name(raw_role.as_str())
            .with_context(|| format!("invalid role name '{}'", raw_role))?;
        if normalized_roles.insert(role.clone(), profile).is_some() {
            bail!("duplicate role '{}' in {}", role, source_label);
        }
    }
    if normalized_roles.is_empty() {
        normalized_roles = default_role_profiles();
    }
    table.roles = normalized_roles;

    normalize_route_target(source_label, &table.roles, &mut table.planner, "planner")?;
    normalize_route_target(
        source_label,
        &table.roles,
        &mut table.delegated,
        "delegated",
    )?;
    normalize_route_target(source_label, &table.roles, &mut table.review, "review")?;

    for (raw_category, target) in &mut table.delegated_categories {
        let category = raw_category.trim();
        if category.is_empty() {
            bail!(
                "delegated route category cannot be empty in {}",
                source_label
            );
        }
        normalize_route_target(
            source_label,
            &table.roles,
            target,
            &format!("delegated_categories['{}']", category),
        )?;
    }

    Ok(())
}

fn normalize_route_target(
    source_label: &str,
    roles: &BTreeMap<String, MultiAgentRoleProfile>,
    target: &mut MultiAgentRouteTarget,
    field_name: &str,
) -> Result<()> {
    let primary = normalize_role_name(target.role.as_str()).with_context(|| {
        format!(
            "invalid role '{}' for route target '{}' in {}",
            target.role, field_name, source_label
        )
    })?;
    if !roles.contains_key(primary.as_str()) {
        bail!(
            "route target '{}' references unknown role '{}' in {}",
            field_name,
            primary,
            source_label
        );
    }

    let mut normalized_fallbacks = Vec::new();
    let mut seen = HashSet::new();
    for raw_role in std::mem::take(&mut target.fallback_roles) {
        let role = normalize_role_name(raw_role.as_str()).with_context(|| {
            format!(
                "invalid fallback role '{}' for route target '{}' in {}",
                raw_role, field_name, source_label
            )
        })?;
        if role == primary {
            continue;
        }
        if !roles.contains_key(role.as_str()) {
            bail!(
                "route target '{}' references unknown fallback role '{}' in {}",
                field_name,
                role,
                source_label
            );
        }
        if seen.insert(role.clone()) {
            normalized_fallbacks.push(role);
        }
    }

    target.role = primary;
    target.fallback_roles = normalized_fallbacks;
    Ok(())
}

fn normalize_role_name(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("role name cannot be empty");
    }
    Ok(trimmed.to_string())
}

pub fn select_multi_agent_route(
    table: &MultiAgentRouteTable,
    phase: MultiAgentRoutePhase,
    step_text: Option<&str>,
) -> MultiAgentRouteSelection {
    let (target, category) = match phase {
        MultiAgentRoutePhase::Planner => (&table.planner, None),
        MultiAgentRoutePhase::Review => (&table.review, None),
        MultiAgentRoutePhase::DelegatedStep => {
            if let Some(step) = step_text {
                if let Some((category, category_target)) =
                    select_delegated_category_target(&table.delegated_categories, step)
                {
                    (category_target, Some(category.to_string()))
                } else {
                    (&table.delegated, None)
                }
            } else {
                (&table.delegated, None)
            }
        }
    };

    let mut attempt_roles = Vec::with_capacity(target.fallback_roles.len() + 1);
    attempt_roles.push(target.role.clone());
    attempt_roles.extend(target.fallback_roles.clone());

    MultiAgentRouteSelection {
        phase,
        category,
        primary_role: target.role.clone(),
        fallback_roles: target.fallback_roles.clone(),
        attempt_roles,
    }
}

fn select_delegated_category_target<'a>(
    categories: &'a BTreeMap<String, MultiAgentRouteTarget>,
    step_text: &str,
) -> Option<(&'a str, &'a MultiAgentRouteTarget)> {
    let normalized = step_text.to_ascii_lowercase();
    categories.iter().find_map(|(category, target)| {
        let category_match = category.trim().to_ascii_lowercase();
        (!category_match.is_empty() && normalized.contains(&category_match))
            .then_some((category.as_str(), target))
    })
}

pub fn resolve_multi_agent_role_profile(
    table: &MultiAgentRouteTable,
    role: &str,
) -> MultiAgentRoleProfile {
    table.roles.get(role).cloned().unwrap_or_default()
}

pub fn build_multi_agent_role_prompt(
    base_prompt: &str,
    phase: MultiAgentRoutePhase,
    role: &str,
    profile: &MultiAgentRoleProfile,
) -> String {
    let suffix = profile
        .prompt_suffix
        .as_deref()
        .map(str::trim)
        .unwrap_or_default();
    let has_explicit_profile = role != DEFAULT_ROLE_NAME
        || profile.model.is_some()
        || profile.tool_policy_preset.is_some()
        || !suffix.is_empty();

    if !has_explicit_profile {
        return base_prompt.to_string();
    }

    let model_hint = profile.model.as_deref().unwrap_or("inherit");
    let tool_policy_hint = profile.tool_policy_preset.as_deref().unwrap_or("inherit");
    if suffix.is_empty() {
        return format!(
            "{base_prompt}\n\nORCHESTRATOR_ROLE_CONTEXT\nphase={}\nrole={}\nmodel_hint={}\ntool_policy_preset={}",
            phase.as_str(),
            role,
            model_hint,
            tool_policy_hint,
        );
    }

    format!(
        "{base_prompt}\n\nORCHESTRATOR_ROLE_CONTEXT\nphase={}\nrole={}\nmodel_hint={}\ntool_policy_preset={}\n\nRole prompt suffix:\n{}",
        phase.as_str(),
        role,
        model_hint,
        tool_policy_hint,
        suffix,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        build_multi_agent_role_prompt, load_multi_agent_route_table, select_multi_agent_route,
        MultiAgentRoleProfile, MultiAgentRoutePhase,
    };
    use tempfile::tempdir;

    #[test]
    fn unit_route_selection_prefers_deterministic_category_match() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("route-table.json");
        std::fs::write(
            &path,
            r#"{
  "schema_version": 1,
  "roles": {
    "planner": {},
    "executor": {},
    "reviewer": {}
  },
  "planner": { "role": "planner" },
  "delegated": { "role": "executor" },
  "delegated_categories": {
    "analysis": { "role": "reviewer" },
    "build": { "role": "executor" }
  },
  "review": { "role": "reviewer" }
}
"#,
        )
        .expect("write route table");

        let table = load_multi_agent_route_table(&path).expect("load route table");
        let route = select_multi_agent_route(
            &table,
            MultiAgentRoutePhase::DelegatedStep,
            Some("do analysis first"),
        );
        assert_eq!(route.primary_role, "reviewer");
        assert_eq!(route.category.as_deref(), Some("analysis"));
    }

    #[test]
    fn unit_route_selection_dedupes_fallback_order() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("route-table.json");
        std::fs::write(
            &path,
            r#"{
  "schema_version": 1,
  "roles": {
    "planner": {},
    "executor": {},
    "reviewer": {}
  },
  "planner": { "role": "planner", "fallback_roles": ["executor", "executor", "planner", "reviewer"] },
  "delegated": { "role": "executor" },
  "review": { "role": "reviewer" }
}
"#,
        )
        .expect("write route table");

        let table = load_multi_agent_route_table(&path).expect("load route table");
        let route = select_multi_agent_route(&table, MultiAgentRoutePhase::Planner, None);
        assert_eq!(route.attempt_roles, vec!["planner", "executor", "reviewer"]);
    }

    #[test]
    fn regression_route_table_validation_rejects_unknown_role_references() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("route-table.json");
        std::fs::write(
            &path,
            r#"{
  "schema_version": 1,
  "roles": {
    "planner": {}
  },
  "planner": { "role": "missing" },
  "delegated": { "role": "planner" },
  "review": { "role": "planner" }
}
"#,
        )
        .expect("write route table");

        let error = load_multi_agent_route_table(&path).expect_err("unknown role should fail");
        assert!(error.to_string().contains("references unknown role"));
    }

    #[test]
    fn regression_role_prompt_builder_keeps_legacy_prompt_for_default_profile() {
        let base = "base prompt";
        let rendered = build_multi_agent_role_prompt(
            base,
            MultiAgentRoutePhase::Planner,
            "default",
            &MultiAgentRoleProfile::default(),
        );
        assert_eq!(rendered, base);
    }

    #[test]
    fn functional_role_prompt_builder_includes_profile_context_and_suffix() {
        let base = "base prompt";
        let rendered = build_multi_agent_role_prompt(
            base,
            MultiAgentRoutePhase::Review,
            "reviewer",
            &MultiAgentRoleProfile {
                model: Some("openai/gpt-4o-mini".to_string()),
                prompt_suffix: Some("Check edge cases.".to_string()),
                tool_policy_preset: Some("balanced".to_string()),
            },
        );
        assert!(rendered.contains("ORCHESTRATOR_ROLE_CONTEXT"));
        assert!(rendered.contains("role=reviewer"));
        assert!(rendered.contains("model_hint=openai/gpt-4o-mini"));
        assert!(rendered.contains("Check edge cases."));
    }
}
