use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

const ROUTE_TABLE_SCHEMA_VERSION: u32 = 1;
const DEFAULT_ROLE_NAME: &str = "default";
const DEFAULT_TRUST_WEIGHT: u16 = 100;
const DEFAULT_TRUST_STALE_AFTER_SECONDS: u64 = 3_600;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
/// Public struct `MultiAgentRoleProfile` used across Tau components.
pub struct MultiAgentRoleProfile {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub prompt_suffix: Option<String>,
    #[serde(default)]
    pub tool_policy_preset: Option<String>,
    #[serde(default)]
    pub trust_weight: Option<u16>,
    #[serde(default)]
    pub minimum_trust_score: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Public struct `MultiAgentRouteTarget` used across Tau components.
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
/// Public struct `MultiAgentRouteTable` used across Tau components.
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
/// Enumerates supported `MultiAgentRoutePhase` values.
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
/// Public struct `MultiAgentRouteSelection` used across Tau components.
pub struct MultiAgentRouteSelection {
    pub phase: MultiAgentRoutePhase,
    pub category: Option<String>,
    pub primary_role: String,
    pub fallback_roles: Vec<String>,
    pub attempt_roles: Vec<String>,
    pub trust_status: String,
    pub trust_score: Option<u8>,
    pub trust_threshold: Option<u8>,
    pub trust_score_source: Option<String>,
    pub trust_stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
/// Optional trust input used for trust-aware route scoring and fallback behavior.
pub struct MultiAgentRouteTrustInput {
    pub global_score: Option<u8>,
    pub role_scores: BTreeMap<String, u8>,
    pub minimum_score: Option<u8>,
    pub updated_unix_ms: Option<u64>,
    pub now_unix_ms: u64,
    pub stale_after_seconds: Option<u64>,
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
    for (raw_role, mut profile) in std::mem::take(&mut table.roles) {
        let role = normalize_role_name(raw_role.as_str())
            .with_context(|| format!("invalid role name '{}'", raw_role))?;
        validate_role_profile_trust(source_label, &role, &mut profile)?;
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

fn validate_role_profile_trust(
    source_label: &str,
    role: &str,
    profile: &mut MultiAgentRoleProfile,
) -> Result<()> {
    if let Some(score) = profile.minimum_trust_score {
        if score > 100 {
            bail!(
                "role '{}' minimum_trust_score {} exceeds 100 in {}",
                role,
                score,
                source_label
            );
        }
    }
    if let Some(weight) = profile.trust_weight {
        if weight == 0 {
            bail!(
                "role '{}' trust_weight must be greater than 0 in {}",
                role,
                source_label
            );
        }
    }
    Ok(())
}

pub fn select_multi_agent_route(
    table: &MultiAgentRouteTable,
    phase: MultiAgentRoutePhase,
    step_text: Option<&str>,
) -> MultiAgentRouteSelection {
    select_multi_agent_route_with_trust(table, phase, step_text, None)
}

pub fn select_multi_agent_route_with_trust(
    table: &MultiAgentRouteTable,
    phase: MultiAgentRoutePhase,
    step_text: Option<&str>,
    trust_input: Option<&MultiAgentRouteTrustInput>,
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

    let mut initial_attempt_roles = Vec::with_capacity(target.fallback_roles.len() + 1);
    initial_attempt_roles.push(target.role.clone());
    initial_attempt_roles.extend(target.fallback_roles.clone());

    let trust_evaluation =
        evaluate_trust_weighted_attempt_roles(table, &initial_attempt_roles, trust_input);

    let primary_role = trust_evaluation
        .attempt_roles
        .first()
        .cloned()
        .unwrap_or_else(|| target.role.clone());
    let fallback_roles = trust_evaluation
        .attempt_roles
        .iter()
        .skip(1)
        .cloned()
        .collect::<Vec<_>>();

    MultiAgentRouteSelection {
        phase,
        category,
        primary_role,
        fallback_roles,
        attempt_roles: trust_evaluation.attempt_roles,
        trust_status: trust_evaluation.status,
        trust_score: trust_evaluation.selected_score,
        trust_threshold: trust_evaluation.selected_threshold,
        trust_score_source: trust_evaluation.selected_source,
        trust_stale: trust_evaluation.stale,
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

#[derive(Debug, Clone)]
struct TrustRouteCandidate {
    role: String,
    original_index: usize,
    threshold: Option<u8>,
    score: Option<u8>,
    raw_score_present: bool,
    weighted_score: u32,
    source: Option<String>,
    meets_threshold: bool,
}

#[derive(Debug, Clone)]
struct TrustRouteEvaluation {
    attempt_roles: Vec<String>,
    status: String,
    selected_score: Option<u8>,
    selected_threshold: Option<u8>,
    selected_source: Option<String>,
    stale: bool,
}

fn evaluate_trust_weighted_attempt_roles(
    table: &MultiAgentRouteTable,
    attempt_roles: &[String],
    trust_input: Option<&MultiAgentRouteTrustInput>,
) -> TrustRouteEvaluation {
    let Some(trust_input) = trust_input else {
        return TrustRouteEvaluation {
            attempt_roles: attempt_roles.to_vec(),
            status: "disabled".to_string(),
            selected_score: None,
            selected_threshold: None,
            selected_source: None,
            stale: false,
        };
    };

    let stale_after_seconds = trust_input
        .stale_after_seconds
        .unwrap_or(DEFAULT_TRUST_STALE_AFTER_SECONDS);
    let stale_after_ms = stale_after_seconds.saturating_mul(1_000);
    let stale = trust_input
        .updated_unix_ms
        .is_some_and(|updated| trust_input.now_unix_ms.saturating_sub(updated) > stale_after_ms);

    let mut candidates = Vec::with_capacity(attempt_roles.len());
    for (index, role) in attempt_roles.iter().enumerate() {
        let profile = resolve_multi_agent_role_profile(table, role);
        let threshold = trust_input.minimum_score.or(profile.minimum_trust_score);
        let weight = u32::from(profile.trust_weight.unwrap_or(DEFAULT_TRUST_WEIGHT));
        let (raw_score, source) = resolve_candidate_score(trust_input, role.as_str());
        let score = if stale { None } else { raw_score };
        let weighted_score = score.map(|value| u32::from(value) * weight).unwrap_or(0);
        let meets_threshold = threshold
            .map(|minimum| score.is_some_and(|value| value >= minimum))
            .unwrap_or(true);

        candidates.push(TrustRouteCandidate {
            role: role.clone(),
            original_index: index,
            threshold,
            score,
            raw_score_present: raw_score.is_some(),
            weighted_score,
            source,
            meets_threshold,
        });
    }

    let has_raw_scores = candidates
        .iter()
        .any(|candidate| candidate.raw_score_present);
    let threshold_gated = candidates
        .iter()
        .any(|candidate| !candidate.meets_threshold);

    let mut eligible = candidates
        .iter()
        .filter(|candidate| candidate.meets_threshold)
        .cloned()
        .collect::<Vec<_>>();
    if !eligible.is_empty() {
        eligible.sort_by(|left, right| {
            right
                .weighted_score
                .cmp(&left.weighted_score)
                .then_with(|| right.score.cmp(&left.score))
                .then_with(|| left.original_index.cmp(&right.original_index))
        });

        let mut ordered_roles = eligible
            .iter()
            .map(|candidate| candidate.role.clone())
            .collect::<Vec<_>>();
        for candidate in candidates
            .iter()
            .filter(|candidate| !candidate.meets_threshold)
        {
            ordered_roles.push(candidate.role.clone());
        }

        let selected = candidates
            .iter()
            .find(|candidate| candidate.role == ordered_roles[0])
            .expect("selected role should exist in candidate set");
        let status = if threshold_gated {
            "threshold_gated".to_string()
        } else if has_raw_scores && !stale {
            "trust_weighted".to_string()
        } else {
            "trust_unweighted".to_string()
        };

        return TrustRouteEvaluation {
            attempt_roles: ordered_roles,
            status,
            selected_score: selected.score,
            selected_threshold: selected.threshold,
            selected_source: selected.source.clone(),
            stale,
        };
    }

    let status = if stale {
        "fallback_stale_trust"
    } else if has_raw_scores {
        "fallback_low_trust"
    } else {
        "fallback_missing_trust"
    };
    let selected = candidates.first();
    TrustRouteEvaluation {
        attempt_roles: attempt_roles.to_vec(),
        status: status.to_string(),
        selected_score: selected.and_then(|candidate| candidate.score),
        selected_threshold: selected.and_then(|candidate| candidate.threshold),
        selected_source: selected.and_then(|candidate| candidate.source.clone()),
        stale,
    }
}

fn resolve_candidate_score(
    trust_input: &MultiAgentRouteTrustInput,
    role: &str,
) -> (Option<u8>, Option<String>) {
    if let Some(score) = trust_input.role_scores.get(role) {
        return (Some(*score), Some("role_scores".to_string()));
    }
    if let Some(score) = trust_input.global_score {
        return (Some(score), Some("global_score".to_string()));
    }
    (None, None)
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
    use std::collections::BTreeMap;

    use super::{
        build_multi_agent_role_prompt, load_multi_agent_route_table, select_multi_agent_route,
        select_multi_agent_route_with_trust, MultiAgentRoleProfile, MultiAgentRoutePhase,
        MultiAgentRouteTrustInput,
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
        assert_eq!(route.trust_status, "disabled");
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
        assert_eq!(route.trust_status, "disabled");
    }

    #[test]
    fn unit_route_selection_with_trust_weights_prefers_higher_weighted_role() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("route-table.json");
        std::fs::write(
            &path,
            r#"{
  "schema_version": 1,
  "roles": {
    "primary": { "trust_weight": 100 },
    "fallback": { "trust_weight": 160 }
  },
  "planner": { "role": "primary", "fallback_roles": ["fallback"] },
  "delegated": { "role": "primary" },
  "review": { "role": "primary" }
}
"#,
        )
        .expect("write route table");

        let table = load_multi_agent_route_table(&path).expect("load route table");
        let trust_input = MultiAgentRouteTrustInput {
            role_scores: BTreeMap::from([
                ("primary".to_string(), 80),
                ("fallback".to_string(), 70),
            ]),
            now_unix_ms: 1_760_200_000_000,
            ..MultiAgentRouteTrustInput::default()
        };
        let route = select_multi_agent_route_with_trust(
            &table,
            MultiAgentRoutePhase::Planner,
            None,
            Some(&trust_input),
        );
        assert_eq!(route.primary_role, "fallback");
        assert_eq!(route.attempt_roles, vec!["fallback", "primary"]);
        assert_eq!(route.trust_status, "trust_weighted");
        assert_eq!(route.trust_score, Some(70));
        assert_eq!(route.trust_score_source.as_deref(), Some("role_scores"));
    }

    #[test]
    fn functional_route_selection_with_threshold_reorders_high_trust_role_first() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("route-table.json");
        std::fs::write(
            &path,
            r#"{
  "schema_version": 1,
  "roles": {
    "primary": { "minimum_trust_score": 90 },
    "fallback": { "minimum_trust_score": 30 }
  },
  "planner": { "role": "primary", "fallback_roles": ["fallback"] },
  "delegated": { "role": "primary" },
  "review": { "role": "primary" }
}
"#,
        )
        .expect("write route table");

        let table = load_multi_agent_route_table(&path).expect("load route table");
        let trust_input = MultiAgentRouteTrustInput {
            role_scores: BTreeMap::from([
                ("primary".to_string(), 60),
                ("fallback".to_string(), 70),
            ]),
            now_unix_ms: 1_760_200_000_000,
            ..MultiAgentRouteTrustInput::default()
        };
        let route = select_multi_agent_route_with_trust(
            &table,
            MultiAgentRoutePhase::Planner,
            None,
            Some(&trust_input),
        );
        assert_eq!(route.primary_role, "fallback");
        assert_eq!(route.attempt_roles, vec!["fallback", "primary"]);
        assert_eq!(route.trust_status, "threshold_gated");
        assert_eq!(route.trust_threshold, Some(30));
    }

    #[test]
    fn regression_route_selection_handles_stale_trust_with_deterministic_fallback() {
        let table = super::MultiAgentRouteTable::default();
        let trust_input = MultiAgentRouteTrustInput {
            global_score: Some(20),
            minimum_score: Some(90),
            updated_unix_ms: Some(1_760_100_000_000),
            now_unix_ms: 1_760_200_000_000,
            stale_after_seconds: Some(60),
            ..MultiAgentRouteTrustInput::default()
        };
        let route = select_multi_agent_route_with_trust(
            &table,
            MultiAgentRoutePhase::Planner,
            None,
            Some(&trust_input),
        );
        assert_eq!(route.primary_role, "default");
        assert_eq!(route.attempt_roles, vec!["default"]);
        assert_eq!(route.trust_status, "fallback_stale_trust");
        assert!(route.trust_stale);
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
                trust_weight: None,
                minimum_trust_score: None,
            },
        );
        assert!(rendered.contains("ORCHESTRATOR_ROLE_CONTEXT"));
        assert!(rendered.contains("role=reviewer"));
        assert!(rendered.contains("model_hint=openai/gpt-4o-mini"));
        assert!(rendered.contains("Check edge cases."));
    }
}
