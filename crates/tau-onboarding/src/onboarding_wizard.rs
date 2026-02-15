use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tau_cli::Cli;
use tau_core::{current_unix_timestamp_ms, write_text_atomic};
use tau_provider::ProviderAuthMethod;

use crate::onboarding_paths::resolve_tau_root;
use crate::startup_config::ProfileDefaults;

const ONBOARDING_BASELINE_SCHEMA_VERSION: u32 = 1;
const ONBOARDING_BASELINE_FILE_NAME: &str = "onboarding-baseline.json";
const IDENTITY_TEMPLATE_SOUL: &str =
    "# SOUL\n\nCore operating values and immutable project principles.\n";
const IDENTITY_TEMPLATE_AGENTS: &str =
    "# AGENTS\n\nOperational contracts, code quality bars, and execution workflow.\n";
const IDENTITY_TEMPLATE_USER: &str =
    "# USER\n\nOperator-specific preferences and collaboration constraints.\n";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Enumerates supported `OnboardingEntrypoint` values.
pub enum OnboardingEntrypoint {
    ExplicitFlag,
    FirstRunAuto,
}

impl OnboardingEntrypoint {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExplicitFlag => "explicit_flag",
            Self::FirstRunAuto => "first_run_auto",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `OnboardingFirstRunState` used across Tau components.
pub struct OnboardingFirstRunState {
    pub is_first_run: bool,
    pub tau_root: String,
    pub profile_store_exists: bool,
    pub release_channel_exists: bool,
    pub reason_codes: Vec<String>,
}

pub fn detect_onboarding_first_run_state(cli: &Cli) -> OnboardingFirstRunState {
    let tau_root = resolve_tau_root(cli);
    let profile_store_exists = tau_root.join("profiles.json").is_file();
    let release_channel_exists = tau_root.join("release-channel.json").is_file();
    let is_first_run = !profile_store_exists && !release_channel_exists;
    let mut reason_codes = Vec::new();
    if is_first_run {
        reason_codes.push("onboarding_first_run_detected".to_string());
    } else {
        if profile_store_exists {
            reason_codes.push("onboarding_profile_store_present".to_string());
        } else {
            reason_codes.push("onboarding_profile_store_missing".to_string());
        }
        if release_channel_exists {
            reason_codes.push("onboarding_release_channel_present".to_string());
        } else {
            reason_codes.push("onboarding_release_channel_missing".to_string());
        }
    }
    OnboardingFirstRunState {
        is_first_run,
        tau_root: tau_root.display().to_string(),
        profile_store_exists,
        release_channel_exists,
        reason_codes,
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Enumerates supported `OnboardingProvider` values.
pub enum OnboardingProvider {
    OpenAi,
    Anthropic,
    Google,
}

impl OnboardingProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Anthropic => "anthropic",
            Self::Google => "google",
        }
    }

    fn default_model(self) -> &'static str {
        match self {
            Self::OpenAi => "openai/gpt-4o-mini",
            Self::Anthropic => "anthropic/claude-sonnet-4-20250514",
            Self::Google => "google/gemini-2.5-pro",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `OnboardingWizardPlan` used across Tau components.
pub struct OnboardingWizardPlan {
    pub selected_provider: OnboardingProvider,
    pub selected_auth_mode: ProviderAuthMethod,
    pub selected_model: String,
    pub selected_workspace: String,
    pub profile_repair_requested: bool,
    pub identity_generation_requested: bool,
    pub identity_repair_requested: bool,
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `OnboardingWizardBaselinePersistResult` used across Tau components.
pub struct OnboardingWizardBaselinePersistResult {
    pub path: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `OnboardingIdentityFileAction` used across Tau components.
pub struct OnboardingIdentityFileAction {
    pub file_name: String,
    pub path: String,
    pub action: String,
    pub reason_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `OnboardingIdentityBootstrapReport` used across Tau components.
pub struct OnboardingIdentityBootstrapReport {
    pub requested: bool,
    pub repair_requested: bool,
    pub files_created: usize,
    pub files_repaired: usize,
    pub files_skipped_existing: usize,
    pub files_skipped_invalid_type: usize,
    pub reason_codes: Vec<String>,
    pub files: Vec<OnboardingIdentityFileAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct OnboardingWizardBaselineFile {
    schema_version: u32,
    generated_at_ms: u64,
    entrypoint: String,
    first_run_detected: bool,
    selected_provider: String,
    selected_auth_mode: String,
    selected_model: String,
    selected_workspace: String,
    profile_repair_requested: bool,
    identity_generation_requested: bool,
    identity_repair_requested: bool,
    reason_codes: Vec<String>,
}

fn infer_provider_from_model(model: &str) -> OnboardingProvider {
    let provider = model
        .split('/')
        .next()
        .unwrap_or("openai")
        .trim()
        .to_ascii_lowercase();
    match provider.as_str() {
        "anthropic" => OnboardingProvider::Anthropic,
        "google" => OnboardingProvider::Google,
        _ => OnboardingProvider::OpenAi,
    }
}

fn configured_auth_mode_for_provider(
    cli: &Cli,
    provider: OnboardingProvider,
) -> ProviderAuthMethod {
    match provider {
        OnboardingProvider::OpenAi => cli.openai_auth_mode.into(),
        OnboardingProvider::Anthropic => cli.anthropic_auth_mode.into(),
        OnboardingProvider::Google => cli.google_auth_mode.into(),
    }
}

fn parse_provider_choice(raw: &str, default: OnboardingProvider) -> Option<OnboardingProvider> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "" => Some(default),
        "1" | "openai" | "openrouter" | "azure" | "azure-openai" | "groq" | "xai" | "mistral" => {
            Some(OnboardingProvider::OpenAi)
        }
        "2" | "anthropic" => Some(OnboardingProvider::Anthropic),
        "3" | "google" | "gemini" => Some(OnboardingProvider::Google),
        _ => None,
    }
}

fn parse_auth_mode_choice(
    raw: &str,
    provider: OnboardingProvider,
    default: ProviderAuthMethod,
) -> Option<ProviderAuthMethod> {
    let normalized = raw
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_")
        .replace(' ', "");
    if normalized.is_empty() {
        return Some(default);
    }
    let parsed = match normalized.as_str() {
        "1" | "apikey" | "api_key" => ProviderAuthMethod::ApiKey,
        "2" | "oauth" | "oauth_token" => ProviderAuthMethod::OauthToken,
        "3" => match provider {
            OnboardingProvider::Google => ProviderAuthMethod::Adc,
            _ => ProviderAuthMethod::SessionToken,
        },
        "adc" => ProviderAuthMethod::Adc,
        "session" | "sessiontoken" | "session_token" => ProviderAuthMethod::SessionToken,
        _ => return None,
    };

    let allowed = provider_auth_modes(provider);
    if allowed.contains(&parsed) {
        Some(parsed)
    } else {
        None
    }
}

fn provider_auth_modes(provider: OnboardingProvider) -> &'static [ProviderAuthMethod] {
    match provider {
        OnboardingProvider::OpenAi | OnboardingProvider::Anthropic => &[
            ProviderAuthMethod::ApiKey,
            ProviderAuthMethod::OauthToken,
            ProviderAuthMethod::SessionToken,
        ],
        OnboardingProvider::Google => &[
            ProviderAuthMethod::ApiKey,
            ProviderAuthMethod::OauthToken,
            ProviderAuthMethod::Adc,
        ],
    }
}

fn normalize_model_choice(provider: OnboardingProvider, raw: &str, fallback: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return fallback.to_string();
    }
    if trimmed.contains('/') {
        return trimmed.to_string();
    }
    format!("{}/{}", provider.as_str(), trimmed)
}

pub fn resolve_onboarding_entrypoint(cli: &Cli) -> OnboardingEntrypoint {
    if cli.onboard {
        OnboardingEntrypoint::ExplicitFlag
    } else {
        OnboardingEntrypoint::FirstRunAuto
    }
}

pub fn resolve_non_interactive_wizard_plan(
    cli: &Cli,
    entrypoint: OnboardingEntrypoint,
    first_run: &OnboardingFirstRunState,
) -> OnboardingWizardPlan {
    let provider = infer_provider_from_model(&cli.model);
    let workspace = default_workspace_from_first_run_state(first_run);
    let mut reason_codes = vec![format!("wizard_entrypoint_{}", entrypoint.as_str())];
    reason_codes.push("wizard_non_interactive_defaults".to_string());
    reason_codes.extend(first_run.reason_codes.iter().cloned());

    OnboardingWizardPlan {
        selected_provider: provider,
        selected_auth_mode: configured_auth_mode_for_provider(cli, provider),
        selected_model: cli.model.clone(),
        selected_workspace: workspace,
        profile_repair_requested: false,
        identity_generation_requested: false,
        identity_repair_requested: false,
        reason_codes,
    }
}

pub fn resolve_interactive_wizard_plan<F>(
    cli: &Cli,
    entrypoint: OnboardingEntrypoint,
    first_run: &OnboardingFirstRunState,
    mut prompt_line: F,
) -> Result<OnboardingWizardPlan>
where
    F: FnMut(&str) -> Result<String>,
{
    let default_provider = infer_provider_from_model(&cli.model);
    let default_workspace = default_workspace_from_first_run_state(first_run);

    let provider_prompt = format!(
        "provider [1=openai,2=anthropic,3=google] (default={}): ",
        default_provider.as_str()
    );
    let provider = parse_provider_choice(&prompt_line(&provider_prompt)?, default_provider)
        .unwrap_or(default_provider);

    let default_auth_mode = configured_auth_mode_for_provider(cli, provider);
    let auth_hint = match provider {
        OnboardingProvider::Google => "1=api_key,2=oauth_token,3=adc",
        _ => "1=api_key,2=oauth_token,3=session_token",
    };
    let auth_prompt = format!(
        "auth mode for {} [{}] (default={}): ",
        provider.as_str(),
        auth_hint,
        default_auth_mode.as_str()
    );
    let selected_auth_mode =
        parse_auth_mode_choice(&prompt_line(&auth_prompt)?, provider, default_auth_mode)
            .unwrap_or(default_auth_mode);

    let default_model = if provider == default_provider {
        cli.model.clone()
    } else {
        provider.default_model().to_string()
    };
    let model_prompt = format!(
        "model for {} (default={}): ",
        provider.as_str(),
        default_model
    );
    let selected_model =
        normalize_model_choice(provider, &prompt_line(&model_prompt)?, &default_model);

    let workspace_prompt = format!("workspace root (default={}): ", default_workspace);
    let selected_workspace = {
        let raw = prompt_line(&workspace_prompt)?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            default_workspace.clone()
        } else {
            trimmed.to_string()
        }
    };

    let profile_repair_requested = if first_run.is_first_run {
        false
    } else {
        parse_yes_no(
            &prompt_line("repair existing profile defaults? [y/N]: ")?,
            false,
        )
    };
    let identity_generation_requested = parse_yes_no(
        &prompt_line("generate identity files (SOUL.md, AGENTS.md, USER.md)? [y/N]: ")?,
        false,
    );
    let identity_repair_requested = if identity_generation_requested && !first_run.is_first_run {
        parse_yes_no(
            &prompt_line("repair/overwrite existing identity files if present? [y/N]: ")?,
            false,
        )
    } else {
        false
    };

    let mut reason_codes = vec![format!("wizard_entrypoint_{}", entrypoint.as_str())];
    reason_codes.push("wizard_interactive_prompts_completed".to_string());
    reason_codes.extend(first_run.reason_codes.iter().cloned());
    if profile_repair_requested {
        reason_codes.push("wizard_profile_repair_requested".to_string());
    }
    if identity_generation_requested {
        reason_codes.push("wizard_identity_generation_requested".to_string());
    }
    if identity_repair_requested {
        reason_codes.push("wizard_identity_repair_requested".to_string());
    }

    Ok(OnboardingWizardPlan {
        selected_provider: provider,
        selected_auth_mode,
        selected_model,
        selected_workspace,
        profile_repair_requested,
        identity_generation_requested,
        identity_repair_requested,
        reason_codes,
    })
}

pub fn apply_wizard_plan_to_profile_defaults(
    mut defaults: ProfileDefaults,
    plan: &OnboardingWizardPlan,
) -> ProfileDefaults {
    defaults.model = plan.selected_model.clone();
    match plan.selected_provider {
        OnboardingProvider::OpenAi => defaults.auth.openai = plan.selected_auth_mode,
        OnboardingProvider::Anthropic => defaults.auth.anthropic = plan.selected_auth_mode,
        OnboardingProvider::Google => defaults.auth.google = plan.selected_auth_mode,
    }
    if defaults.session.enabled {
        let session_path = Path::new(&plan.selected_workspace)
            .join(".tau")
            .join("sessions")
            .join("default.sqlite");
        defaults.session.path = Some(session_path.display().to_string());
    }
    defaults
}

pub fn persist_onboarding_baseline(
    tau_root: &Path,
    entrypoint: OnboardingEntrypoint,
    first_run: &OnboardingFirstRunState,
    plan: &OnboardingWizardPlan,
) -> Result<OnboardingWizardBaselinePersistResult> {
    let baseline_path = tau_root.join(ONBOARDING_BASELINE_FILE_NAME);
    let baseline_existed = baseline_path.exists();
    let payload = OnboardingWizardBaselineFile {
        schema_version: ONBOARDING_BASELINE_SCHEMA_VERSION,
        generated_at_ms: current_unix_timestamp_ms(),
        entrypoint: entrypoint.as_str().to_string(),
        first_run_detected: first_run.is_first_run,
        selected_provider: plan.selected_provider.as_str().to_string(),
        selected_auth_mode: plan.selected_auth_mode.as_str().to_string(),
        selected_model: plan.selected_model.clone(),
        selected_workspace: plan.selected_workspace.clone(),
        profile_repair_requested: plan.profile_repair_requested,
        identity_generation_requested: plan.identity_generation_requested,
        identity_repair_requested: plan.identity_repair_requested,
        reason_codes: plan.reason_codes.clone(),
    };
    let mut encoded = serde_json::to_string_pretty(&payload)?;
    encoded.push('\n');
    write_text_atomic(&baseline_path, &encoded)?;

    Ok(OnboardingWizardBaselinePersistResult {
        path: baseline_path.display().to_string(),
        action: if baseline_existed {
            "updated".to_string()
        } else {
            "created".to_string()
        },
    })
}

pub fn bootstrap_identity_files(
    tau_root: &Path,
    plan: &OnboardingWizardPlan,
) -> Result<OnboardingIdentityBootstrapReport> {
    if !plan.identity_generation_requested {
        return Ok(OnboardingIdentityBootstrapReport {
            requested: false,
            repair_requested: false,
            files_created: 0,
            files_repaired: 0,
            files_skipped_existing: 0,
            files_skipped_invalid_type: 0,
            reason_codes: vec!["identity_bootstrap_not_requested".to_string()],
            files: Vec::new(),
        });
    }

    let specs = [
        ("SOUL.md", IDENTITY_TEMPLATE_SOUL),
        ("AGENTS.md", IDENTITY_TEMPLATE_AGENTS),
        ("USER.md", IDENTITY_TEMPLATE_USER),
    ];
    let mut report = OnboardingIdentityBootstrapReport {
        requested: true,
        repair_requested: plan.identity_repair_requested,
        files_created: 0,
        files_repaired: 0,
        files_skipped_existing: 0,
        files_skipped_invalid_type: 0,
        reason_codes: Vec::new(),
        files: Vec::new(),
    };

    for (file_name, template) in specs {
        let path = tau_root.join(file_name);
        let action = if path.exists() {
            if !path.is_file() {
                report.files_skipped_invalid_type += 1;
                push_unique_reason(&mut report.reason_codes, "identity_file_invalid_type");
                OnboardingIdentityFileAction {
                    file_name: file_name.to_string(),
                    path: path.display().to_string(),
                    action: "skipped".to_string(),
                    reason_code: "identity_file_invalid_type".to_string(),
                }
            } else if plan.identity_repair_requested {
                write_text_atomic(&path, template)?;
                report.files_repaired += 1;
                push_unique_reason(&mut report.reason_codes, "identity_file_repaired");
                OnboardingIdentityFileAction {
                    file_name: file_name.to_string(),
                    path: path.display().to_string(),
                    action: "repaired".to_string(),
                    reason_code: "identity_file_repaired".to_string(),
                }
            } else {
                report.files_skipped_existing += 1;
                push_unique_reason(&mut report.reason_codes, "identity_file_existing_preserved");
                OnboardingIdentityFileAction {
                    file_name: file_name.to_string(),
                    path: path.display().to_string(),
                    action: "skipped".to_string(),
                    reason_code: "identity_file_existing_preserved".to_string(),
                }
            }
        } else {
            write_text_atomic(&path, template)?;
            report.files_created += 1;
            push_unique_reason(&mut report.reason_codes, "identity_file_created");
            OnboardingIdentityFileAction {
                file_name: file_name.to_string(),
                path: path.display().to_string(),
                action: "created".to_string(),
                reason_code: "identity_file_created".to_string(),
            }
        };
        report.files.push(action);
    }

    if report.reason_codes.is_empty() {
        report
            .reason_codes
            .push("identity_bootstrap_clean".to_string());
    }
    Ok(report)
}

fn parse_yes_no(raw: &str, default_yes: bool) -> bool {
    match raw.trim().to_ascii_lowercase().as_str() {
        "" => default_yes,
        "y" | "yes" => true,
        "n" | "no" => false,
        _ => default_yes,
    }
}

fn push_unique_reason(reason_codes: &mut Vec<String>, reason: &str) {
    if reason_codes.iter().any(|existing| existing == reason) {
        return;
    }
    reason_codes.push(reason.to_string());
}

fn default_workspace_from_first_run_state(first_run: &OnboardingFirstRunState) -> String {
    let tau_root = Path::new(&first_run.tau_root);
    if let Some(parent) = tau_root
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        return parent.display().to_string();
    }
    std::env::current_dir()
        .ok()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| ".".to_string())
}

pub fn resolve_onboarding_workspace_tau_root(plan: &OnboardingWizardPlan, cli: &Cli) -> PathBuf {
    if plan.selected_workspace.trim().is_empty() {
        return resolve_tau_root(cli);
    }
    Path::new(&plan.selected_workspace).join(".tau")
}

#[cfg(test)]
mod tests {
    use super::{
        apply_wizard_plan_to_profile_defaults, bootstrap_identity_files,
        detect_onboarding_first_run_state, persist_onboarding_baseline,
        resolve_interactive_wizard_plan, resolve_non_interactive_wizard_plan,
        resolve_onboarding_workspace_tau_root, OnboardingEntrypoint, OnboardingFirstRunState,
        OnboardingProvider, OnboardingWizardPlan,
    };
    use clap::Parser;
    use std::path::Path;
    use tau_cli::Cli;
    use tau_provider::ProviderAuthMethod;
    use tempfile::tempdir;

    fn parse_cli_with_stack() -> Cli {
        std::thread::Builder::new()
            .name("tau-cli-parse".to_string())
            .stack_size(16 * 1024 * 1024)
            .spawn(|| Cli::parse_from(["tau-rs"]))
            .expect("spawn cli parse thread")
            .join()
            .expect("join cli parse thread")
    }

    fn apply_workspace_paths(cli: &mut Cli, workspace: &Path) {
        let tau_root = workspace.join(".tau");
        cli.session = tau_root.join("sessions/default.sqlite");
        cli.credential_store = tau_root.join("credentials.json");
        cli.skills_dir = tau_root.join("skills");
        cli.model_catalog_cache = tau_root.join("models/catalog.json");
        cli.channel_store_root = tau_root.join("channel-store");
        cli.events_dir = tau_root.join("events");
        cli.events_state_path = tau_root.join("events/state.json");
        cli.dashboard_state_dir = tau_root.join("dashboard");
        cli.github_state_dir = tau_root.join("github-issues");
        cli.slack_state_dir = tau_root.join("slack");
        cli.package_install_root = tau_root.join("packages");
        cli.package_update_root = tau_root.join("packages");
        cli.package_list_root = tau_root.join("packages");
        cli.package_remove_root = tau_root.join("packages");
        cli.package_rollback_root = tau_root.join("packages");
        cli.package_conflicts_root = tau_root.join("packages");
        cli.package_activate_root = tau_root.join("packages");
        cli.package_activate_destination = tau_root.join("packages-active");
        cli.extension_list_root = tau_root.join("extensions");
        cli.extension_runtime_root = tau_root.join("extensions");
    }

    fn sample_first_run_state() -> OnboardingFirstRunState {
        OnboardingFirstRunState {
            is_first_run: true,
            tau_root: ".tau".to_string(),
            profile_store_exists: false,
            release_channel_exists: false,
            reason_codes: vec!["onboarding_first_run_detected".to_string()],
        }
    }

    #[test]
    fn unit_detect_onboarding_first_run_state_detects_missing_stores() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        apply_workspace_paths(&mut cli, temp.path());
        let state = detect_onboarding_first_run_state(&cli);
        assert!(state.is_first_run);
        assert!(state
            .reason_codes
            .contains(&"onboarding_first_run_detected".to_string()));
    }

    #[test]
    fn functional_resolve_interactive_wizard_plan_parses_guided_choices() {
        let cli = parse_cli_with_stack();
        let first_run = sample_first_run_state();
        let mut answers = vec![
            "2".to_string(),
            "2".to_string(),
            "claude-sonnet-4-20250514".to_string(),
            "/tmp/workspace".to_string(),
            "y".to_string(),
        ]
        .into_iter();
        let plan = resolve_interactive_wizard_plan(
            &cli,
            OnboardingEntrypoint::ExplicitFlag,
            &first_run,
            |_prompt| Ok(answers.next().unwrap_or_default()),
        )
        .expect("interactive plan");
        assert_eq!(plan.selected_provider, OnboardingProvider::Anthropic);
        assert_eq!(plan.selected_auth_mode, ProviderAuthMethod::OauthToken);
        assert_eq!(
            plan.selected_model,
            "anthropic/claude-sonnet-4-20250514".to_string()
        );
        assert_eq!(plan.selected_workspace, "/tmp/workspace".to_string());
        assert!(plan.identity_generation_requested);
    }

    #[test]
    fn integration_persist_onboarding_baseline_and_identity_files_round_trip() {
        let temp = tempdir().expect("tempdir");
        let tau_root = temp.path().join(".tau");
        std::fs::create_dir_all(&tau_root).expect("create tau root");
        let plan = OnboardingWizardPlan {
            selected_provider: OnboardingProvider::OpenAi,
            selected_auth_mode: ProviderAuthMethod::ApiKey,
            selected_model: "openai/gpt-4o-mini".to_string(),
            selected_workspace: temp.path().display().to_string(),
            profile_repair_requested: false,
            identity_generation_requested: true,
            identity_repair_requested: false,
            reason_codes: vec!["wizard_interactive_prompts_completed".to_string()],
        };
        let first_run = sample_first_run_state();

        let baseline = persist_onboarding_baseline(
            &tau_root,
            OnboardingEntrypoint::ExplicitFlag,
            &first_run,
            &plan,
        )
        .expect("baseline");
        assert_eq!(baseline.action, "created");
        assert!(Path::new(&baseline.path).exists());

        let identity = bootstrap_identity_files(&tau_root, &plan).expect("identity");
        assert_eq!(identity.files_created, 3);
        assert_eq!(identity.files.len(), 3);
    }

    #[test]
    fn regression_bootstrap_identity_files_preserves_existing_when_repair_disabled() {
        let temp = tempdir().expect("tempdir");
        let tau_root = temp.path().join(".tau");
        std::fs::create_dir_all(&tau_root).expect("create tau root");
        std::fs::write(tau_root.join("SOUL.md"), "custom").expect("write soul");
        let plan = OnboardingWizardPlan {
            selected_provider: OnboardingProvider::OpenAi,
            selected_auth_mode: ProviderAuthMethod::ApiKey,
            selected_model: "openai/gpt-4o-mini".to_string(),
            selected_workspace: temp.path().display().to_string(),
            profile_repair_requested: false,
            identity_generation_requested: true,
            identity_repair_requested: false,
            reason_codes: vec![],
        };
        let report = bootstrap_identity_files(&tau_root, &plan).expect("identity");
        assert_eq!(report.files_skipped_existing, 1);
        let soul = std::fs::read_to_string(tau_root.join("SOUL.md")).expect("read soul");
        assert_eq!(soul, "custom");
    }

    #[test]
    fn regression_apply_wizard_plan_to_profile_defaults_updates_selected_provider_and_workspace() {
        let mut cli = parse_cli_with_stack();
        cli.model = "openai/gpt-4o-mini".to_string();
        let defaults = crate::startup_config::build_profile_defaults(&cli);
        let plan = OnboardingWizardPlan {
            selected_provider: OnboardingProvider::Google,
            selected_auth_mode: ProviderAuthMethod::Adc,
            selected_model: "google/gemini-2.5-pro".to_string(),
            selected_workspace: "/tmp/tau-workspace".to_string(),
            profile_repair_requested: false,
            identity_generation_requested: false,
            identity_repair_requested: false,
            reason_codes: vec![],
        };
        let updated = apply_wizard_plan_to_profile_defaults(defaults, &plan);
        assert_eq!(updated.model, "google/gemini-2.5-pro".to_string());
        assert_eq!(updated.auth.google, ProviderAuthMethod::Adc);
        assert_eq!(
            updated.session.path.as_deref(),
            Some("/tmp/tau-workspace/.tau/sessions/default.sqlite")
        );
    }

    #[test]
    fn unit_resolve_non_interactive_wizard_plan_uses_cli_defaults() {
        let mut cli = parse_cli_with_stack();
        cli.model = "google/gemini-2.5-pro".to_string();
        cli.google_auth_mode = tau_cli::CliProviderAuthMode::Adc;
        let first_run = sample_first_run_state();
        let plan = resolve_non_interactive_wizard_plan(
            &cli,
            OnboardingEntrypoint::ExplicitFlag,
            &first_run,
        );
        assert_eq!(plan.selected_provider, OnboardingProvider::Google);
        assert_eq!(plan.selected_auth_mode, ProviderAuthMethod::Adc);
        assert_eq!(plan.selected_model, "google/gemini-2.5-pro".to_string());
    }

    #[test]
    fn functional_resolve_onboarding_workspace_tau_root_uses_plan_workspace() {
        let cli = parse_cli_with_stack();
        let plan = OnboardingWizardPlan {
            selected_provider: OnboardingProvider::OpenAi,
            selected_auth_mode: ProviderAuthMethod::ApiKey,
            selected_model: "openai/gpt-4o-mini".to_string(),
            selected_workspace: "/tmp/custom-root".to_string(),
            profile_repair_requested: false,
            identity_generation_requested: false,
            identity_repair_requested: false,
            reason_codes: vec![],
        };
        let tau_root = resolve_onboarding_workspace_tau_root(&plan, &cli);
        assert_eq!(tau_root, Path::new("/tmp/custom-root/.tau"));
    }
}
