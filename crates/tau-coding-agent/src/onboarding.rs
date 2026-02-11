use super::*;

use std::collections::BTreeSet;
use std::io::{self, Write};

use crate::cli_executable::is_executable_available;
use crate::daemon_runtime::{
    inspect_tau_daemon, install_tau_daemon, start_tau_daemon, TauDaemonConfig,
    TauDaemonStatusReport,
};
use crate::macro_profile_commands::{
    load_profile_store, save_profile_store, validate_profile_name,
};
use crate::release_channel_commands::{save_release_channel_store, ReleaseChannel};
use tau_onboarding::onboarding_paths::{
    collect_bootstrap_directories, parse_yes_no_response, resolve_tau_root,
};

const ONBOARDING_REPORT_SCHEMA_VERSION: u32 = 2;
const ONBOARDING_DEFAULT_PROFILE: &str = "default";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct OnboardingExecutableCheck {
    pub(crate) integration: String,
    pub(crate) executable: String,
    pub(crate) available: bool,
    pub(crate) required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct OnboardingReport {
    pub(crate) schema_version: u32,
    pub(crate) generated_at_ms: u64,
    pub(crate) mode: String,
    pub(crate) tau_root: String,
    pub(crate) profile_name: String,
    pub(crate) profile_store_path: String,
    pub(crate) profile_store_action: String,
    pub(crate) release_channel_path: String,
    pub(crate) release_channel: String,
    pub(crate) release_channel_source: String,
    pub(crate) release_channel_action: String,
    pub(crate) directories_created: Vec<String>,
    pub(crate) directories_existing: Vec<String>,
    pub(crate) files_created: Vec<String>,
    pub(crate) files_existing: Vec<String>,
    pub(crate) executable_checks: Vec<OnboardingExecutableCheck>,
    pub(crate) daemon_bootstrap: OnboardingDaemonBootstrapReport,
    pub(crate) next_steps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct OnboardingDaemonBootstrapReport {
    pub(crate) requested_install: bool,
    pub(crate) requested_start: bool,
    pub(crate) install_action: String,
    pub(crate) start_action: String,
    pub(crate) ready: bool,
    pub(crate) readiness_reason_codes: Vec<String>,
    pub(crate) status: TauDaemonStatusReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OnboardingMode {
    Interactive,
    NonInteractive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OnboardingReleaseChannelState {
    channel: ReleaseChannel,
    source: &'static str,
    action: &'static str,
}

pub(crate) fn execute_onboarding_command(cli: &Cli) -> Result<()> {
    let mode = if cli.onboard_non_interactive {
        OnboardingMode::NonInteractive
    } else {
        OnboardingMode::Interactive
    };
    let profile_name = resolve_onboarding_profile_name(&cli.onboard_profile)?;
    if mode == OnboardingMode::Interactive {
        let prompt = format!(
            "onboarding wizard: profile={} continue? [Y/n]: ",
            profile_name
        );
        if !prompt_yes_no(&prompt, true)? {
            println!("onboarding canceled: no changes applied");
            return Ok(());
        }
    }

    let report = build_onboarding_report(cli, &profile_name, mode)?;
    let report_path = write_onboarding_report(&report, resolve_onboarding_report_path(cli)?)
        .context("failed to persist onboarding report")?;
    println!("{}", render_onboarding_summary(&report, &report_path));
    Ok(())
}

fn resolve_onboarding_profile_name(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    let profile_name = if trimmed.is_empty() {
        ONBOARDING_DEFAULT_PROFILE.to_string()
    } else {
        trimmed.to_string()
    };
    validate_profile_name(&profile_name)?;
    Ok(profile_name)
}

fn build_onboarding_report(
    cli: &Cli,
    profile_name: &str,
    mode: OnboardingMode,
) -> Result<OnboardingReport> {
    let tau_root = resolve_tau_root(cli);
    let directories = collect_bootstrap_directories(cli, &tau_root);
    let mut directories_created = Vec::new();
    let mut directories_existing = Vec::new();
    for directory in directories {
        ensure_directory(
            &directory,
            &mut directories_created,
            &mut directories_existing,
        )?;
    }

    let profile_store_path = tau_root.join("profiles.json");
    let profile_store_action = ensure_profile_store_entry(cli, &profile_store_path, profile_name)?;
    let release_channel_path = tau_root.join("release-channel.json");
    let release_channel_state = ensure_onboarding_release_channel(
        &release_channel_path,
        cli.onboard_release_channel.as_deref(),
    )?;
    let daemon_bootstrap = run_onboarding_daemon_bootstrap(cli)?;
    let mut files_created = Vec::new();
    let mut files_existing = Vec::new();
    if profile_store_action == "created" {
        files_created.push(profile_store_path.display().to_string());
    } else {
        files_existing.push(profile_store_path.display().to_string());
    }
    if release_channel_state.action == "created" {
        files_created.push(release_channel_path.display().to_string());
    } else {
        files_existing.push(release_channel_path.display().to_string());
    }

    let executable_checks = collect_executable_checks(cli);
    let next_steps = build_onboarding_next_steps(cli, &executable_checks, &daemon_bootstrap);

    Ok(OnboardingReport {
        schema_version: ONBOARDING_REPORT_SCHEMA_VERSION,
        generated_at_ms: current_unix_timestamp_ms(),
        mode: onboarding_mode_label(mode).to_string(),
        tau_root: tau_root.display().to_string(),
        profile_name: profile_name.to_string(),
        profile_store_path: profile_store_path.display().to_string(),
        profile_store_action: profile_store_action.to_string(),
        release_channel_path: release_channel_path.display().to_string(),
        release_channel: release_channel_state.channel.as_str().to_string(),
        release_channel_source: release_channel_state.source.to_string(),
        release_channel_action: release_channel_state.action.to_string(),
        directories_created,
        directories_existing,
        files_created,
        files_existing,
        executable_checks,
        daemon_bootstrap,
        next_steps,
    })
}

fn run_onboarding_daemon_bootstrap(cli: &Cli) -> Result<OnboardingDaemonBootstrapReport> {
    let config = TauDaemonConfig {
        state_dir: cli.daemon_state_dir.clone(),
        profile: cli.daemon_profile,
    };

    let requested_install = cli.onboard_install_daemon;
    let requested_start = cli.onboard_start_daemon;
    let install_action = if requested_install {
        install_tau_daemon(&config).with_context(|| {
            format!(
                "onboarding daemon install failed for '{}'; run --daemon-install to retry",
                config.state_dir.display()
            )
        })?;
        "installed"
    } else {
        "skipped"
    };

    let start_action = if requested_start {
        start_tau_daemon(&config).with_context(|| {
            format!(
                "onboarding daemon start failed for '{}'; run --daemon-start after resolving diagnostics",
                config.state_dir.display()
            )
        })?;
        "started"
    } else {
        "skipped"
    };

    let status = inspect_tau_daemon(&config).with_context(|| {
        format!(
            "onboarding daemon readiness inspection failed for '{}'",
            config.state_dir.display()
        )
    })?;

    let mut readiness_reason_codes = BTreeSet::new();
    for diagnostic in &status.diagnostics {
        readiness_reason_codes.insert(diagnostic.clone());
    }
    if requested_install && !status.installed {
        readiness_reason_codes.insert("daemon_install_expected_installed".to_string());
    }
    if requested_start && !status.running {
        readiness_reason_codes.insert("daemon_start_expected_running".to_string());
    }
    let readiness_reason_codes = readiness_reason_codes.into_iter().collect::<Vec<_>>();
    let ready = readiness_reason_codes.is_empty();

    Ok(OnboardingDaemonBootstrapReport {
        requested_install,
        requested_start,
        install_action: install_action.to_string(),
        start_action: start_action.to_string(),
        ready,
        readiness_reason_codes,
        status,
    })
}

fn onboarding_mode_label(mode: OnboardingMode) -> &'static str {
    match mode {
        OnboardingMode::Interactive => "interactive",
        OnboardingMode::NonInteractive => "non-interactive",
    }
}

fn ensure_directory(
    path: &Path,
    directories_created: &mut Vec<String>,
    directories_existing: &mut Vec<String>,
) -> Result<()> {
    if path.exists() {
        if !path.is_dir() {
            bail!(
                "onboarding path '{}' exists but is not a directory",
                path.display()
            );
        }
        directories_existing.push(path.display().to_string());
    } else {
        std::fs::create_dir_all(path)
            .with_context(|| format!("failed to create directory {}", path.display()))?;
        directories_created.push(path.display().to_string());
    }
    Ok(())
}

fn ensure_profile_store_entry(
    cli: &Cli,
    profile_store_path: &Path,
    profile_name: &str,
) -> Result<&'static str> {
    let mut profiles = load_profile_store(profile_store_path)?;
    if profiles.contains_key(profile_name) {
        return Ok("unchanged");
    }

    let file_existed = profile_store_path.exists();
    profiles.insert(profile_name.to_string(), build_profile_defaults(cli));
    save_profile_store(profile_store_path, &profiles)?;
    if file_existed {
        Ok("updated")
    } else {
        Ok("created")
    }
}

fn resolve_onboarding_release_channel_override(
    raw: Option<&str>,
) -> Result<Option<ReleaseChannel>> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    Ok(Some(trimmed.parse::<ReleaseChannel>()?))
}

fn ensure_onboarding_release_channel(
    release_channel_path: &Path,
    override_channel_raw: Option<&str>,
) -> Result<OnboardingReleaseChannelState> {
    let override_channel = resolve_onboarding_release_channel_override(override_channel_raw)?;
    let existing = load_release_channel_store(release_channel_path)?;

    match (override_channel, existing) {
        (Some(channel), Some(existing_channel)) if channel == existing_channel => {
            Ok(OnboardingReleaseChannelState {
                channel,
                source: "override",
                action: "unchanged",
            })
        }
        (Some(channel), Some(_)) => {
            save_release_channel_store(release_channel_path, channel)?;
            Ok(OnboardingReleaseChannelState {
                channel,
                source: "override",
                action: "updated",
            })
        }
        (Some(channel), None) => {
            save_release_channel_store(release_channel_path, channel)?;
            Ok(OnboardingReleaseChannelState {
                channel,
                source: "override",
                action: "created",
            })
        }
        (None, Some(channel)) => Ok(OnboardingReleaseChannelState {
            channel,
            source: "existing",
            action: "unchanged",
        }),
        (None, None) => {
            save_release_channel_store(release_channel_path, ReleaseChannel::Stable)?;
            Ok(OnboardingReleaseChannelState {
                channel: ReleaseChannel::Stable,
                source: "default",
                action: "created",
            })
        }
    }
}

fn collect_executable_checks(cli: &Cli) -> Vec<OnboardingExecutableCheck> {
    let openai_required = cli.openai_codex_backend
        && matches!(
            cli.openai_auth_mode,
            CliProviderAuthMode::OauthToken | CliProviderAuthMode::SessionToken
        );
    let anthropic_required = cli.anthropic_claude_backend
        && matches!(
            cli.anthropic_auth_mode,
            CliProviderAuthMode::OauthToken | CliProviderAuthMode::SessionToken
        );
    let google_required = cli.google_gemini_backend
        && matches!(
            cli.google_auth_mode,
            CliProviderAuthMode::OauthToken | CliProviderAuthMode::Adc
        );
    let gcloud_required = matches!(cli.google_auth_mode, CliProviderAuthMode::Adc);

    vec![
        onboarding_executable_check("openai-codex", &cli.openai_codex_cli, openai_required),
        onboarding_executable_check(
            "anthropic-claude",
            &cli.anthropic_claude_cli,
            anthropic_required,
        ),
        onboarding_executable_check("google-gemini", &cli.google_gemini_cli, google_required),
        onboarding_executable_check("google-gcloud", &cli.google_gcloud_cli, gcloud_required),
    ]
}

fn onboarding_executable_check(
    integration: &str,
    executable: &str,
    required: bool,
) -> OnboardingExecutableCheck {
    OnboardingExecutableCheck {
        integration: integration.to_string(),
        executable: executable.to_string(),
        available: is_executable_available(executable),
        required,
    }
}

fn build_onboarding_next_steps(
    cli: &Cli,
    executable_checks: &[OnboardingExecutableCheck],
    daemon_bootstrap: &OnboardingDaemonBootstrapReport,
) -> Vec<String> {
    let mut next_steps = Vec::new();
    for check in executable_checks {
        if check.required && !check.available {
            next_steps.push(format!(
                "Install or configure '{}' for {} auth workflows.",
                check.executable, check.integration
            ));
        }
    }
    if daemon_bootstrap.requested_install && !daemon_bootstrap.status.installed {
        next_steps.push(format!(
            "Retry daemon install: cargo run -p tau-coding-agent -- --daemon-install --daemon-state-dir {}",
            cli.daemon_state_dir.display()
        ));
    }
    if daemon_bootstrap.requested_start && !daemon_bootstrap.status.running {
        next_steps.push(format!(
            "Retry daemon start: cargo run -p tau-coding-agent -- --daemon-start --daemon-state-dir {}",
            cli.daemon_state_dir.display()
        ));
    }
    if !daemon_bootstrap.ready {
        next_steps.push(format!(
            "Inspect daemon diagnostics: cargo run -p tau-coding-agent -- --daemon-status --daemon-status-json --daemon-state-dir {}",
            cli.daemon_state_dir.display()
        ));
    }
    next_steps.push("/auth status".to_string());
    next_steps.push(format!(
        "cargo run -p tau-coding-agent -- --model {}",
        cli.model
    ));
    next_steps
}

fn resolve_onboarding_report_path(cli: &Cli) -> Result<PathBuf> {
    let tau_root = resolve_tau_root(cli);
    let reports_dir = tau_root.join("reports");
    let report_name = format!("onboarding-{}.json", current_unix_timestamp_ms());
    Ok(reports_dir.join(report_name))
}

fn write_onboarding_report(report: &OnboardingReport, report_path: PathBuf) -> Result<PathBuf> {
    let mut payload = serde_json::to_string_pretty(report).context("failed to encode report")?;
    payload.push('\n');
    write_text_atomic(&report_path, &payload).with_context(|| {
        format!(
            "failed to write onboarding report {}",
            report_path.display()
        )
    })?;
    Ok(report_path)
}

fn render_onboarding_summary(report: &OnboardingReport, report_path: &Path) -> String {
    let mut lines = vec![
        format!(
            "onboarding complete: mode={} profile={} report={}",
            report.mode,
            report.profile_name,
            report_path.display()
        ),
        format!(
            "directories: created={} existing={}",
            report.directories_created.len(),
            report.directories_existing.len()
        ),
        format!(
            "files: created={} existing={} profile_store_action={}",
            report.files_created.len(),
            report.files_existing.len(),
            report.profile_store_action
        ),
        format!(
            "release_channel: value={} source={} action={} path={}",
            report.release_channel,
            report.release_channel_source,
            report.release_channel_action,
            report.release_channel_path
        ),
        format!(
            "daemon: install_requested={} start_requested={} install_action={} start_action={} ready={}",
            report.daemon_bootstrap.requested_install,
            report.daemon_bootstrap.requested_start,
            report.daemon_bootstrap.install_action,
            report.daemon_bootstrap.start_action,
            report.daemon_bootstrap.ready
        ),
        format!(
            "daemon_status: installed={} running={} profile={} diagnostics={}",
            report.daemon_bootstrap.status.installed,
            report.daemon_bootstrap.status.running,
            report.daemon_bootstrap.status.profile,
            report.daemon_bootstrap.status.diagnostics.len()
        ),
    ];
    for reason in &report.daemon_bootstrap.readiness_reason_codes {
        lines.push(format!("daemon_reason: {reason}"));
    }
    for next_step in &report.next_steps {
        lines.push(format!("next: {next_step}"));
    }
    lines.join("\n")
}

fn prompt_yes_no(prompt: &str, default_yes: bool) -> Result<bool> {
    print!("{prompt}");
    io::stdout()
        .flush()
        .context("failed to flush onboarding prompt")?;
    let mut buffer = String::new();
    io::stdin()
        .read_line(&mut buffer)
        .context("failed to read onboarding prompt response")?;
    Ok(parse_yes_no_response(&buffer, default_yes))
}

#[cfg(test)]
mod tests {
    use super::{
        build_onboarding_report, parse_yes_no_response, resolve_tau_root, OnboardingMode,
        ONBOARDING_REPORT_SCHEMA_VERSION,
    };
    use crate::Cli;
    use clap::Parser;
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;

    fn parse_cli_with_stack() -> Cli {
        std::thread::Builder::new()
            .name("tau-cli-parse".to_string())
            .stack_size(16 * 1024 * 1024)
            .spawn(|| Cli::parse_from(["tau-rs", "--onboard", "--onboard-non-interactive"]))
            .expect("spawn cli parse thread")
            .join()
            .expect("join cli parse thread")
    }

    fn test_cli() -> Cli {
        parse_cli_with_stack()
    }

    fn apply_workspace_paths(cli: &mut Cli, workspace: &Path) {
        let tau_root = workspace.join(".tau");
        cli.session = tau_root.join("sessions/default.jsonl");
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
        cli.daemon_state_dir = tau_root.join("daemon");
    }

    #[test]
    fn unit_parse_yes_no_response_accepts_supported_values() {
        assert!(parse_yes_no_response("yes", false));
        assert!(parse_yes_no_response("Y", false));
        assert!(!parse_yes_no_response("n", true));
        assert!(!parse_yes_no_response("no", true));
        assert!(parse_yes_no_response("", true));
        assert!(!parse_yes_no_response("", false));
    }

    #[test]
    fn functional_resolve_tau_root_prefers_sessions_parent() {
        let mut cli = test_cli();
        let temp = tempdir().expect("tempdir");
        apply_workspace_paths(&mut cli, temp.path());
        let tau_root = resolve_tau_root(&cli);
        assert_eq!(tau_root, temp.path().join(".tau"));
    }

    #[test]
    fn integration_build_onboarding_report_bootstraps_workspace_and_profile_store() {
        let temp = tempdir().expect("tempdir");
        let mut cli = test_cli();
        apply_workspace_paths(&mut cli, temp.path());
        cli.onboard_profile = "team-default".to_string();

        let report = build_onboarding_report(&cli, "team-default", OnboardingMode::NonInteractive)
            .expect("build report");

        assert_eq!(report.schema_version, ONBOARDING_REPORT_SCHEMA_VERSION);
        assert_eq!(report.profile_name, "team-default");
        assert!(!report.directories_created.is_empty());
        assert_eq!(report.profile_store_action, "created");
        assert_eq!(report.release_channel, "stable");
        assert_eq!(report.release_channel_source, "default");
        assert_eq!(report.release_channel_action, "created");
        assert!(!report.daemon_bootstrap.requested_install);
        assert!(!report.daemon_bootstrap.requested_start);
        assert_eq!(report.daemon_bootstrap.install_action, "skipped");
        assert_eq!(report.daemon_bootstrap.start_action, "skipped");
        assert!(report.daemon_bootstrap.ready);
        assert!(
            PathBuf::from(&report.profile_store_path).exists(),
            "profile store should exist after onboarding"
        );
        assert!(
            PathBuf::from(&report.release_channel_path).exists(),
            "release channel store should exist after onboarding"
        );
    }

    #[test]
    fn regression_build_onboarding_report_does_not_overwrite_existing_profile_entry() {
        let temp = tempdir().expect("tempdir");
        let mut cli = test_cli();
        apply_workspace_paths(&mut cli, temp.path());
        cli.onboard_profile = "default".to_string();

        let first = build_onboarding_report(&cli, "default", OnboardingMode::NonInteractive)
            .expect("first report");
        assert_eq!(first.profile_store_action, "created");

        let second = build_onboarding_report(&cli, "default", OnboardingMode::NonInteractive)
            .expect("second report");
        assert_eq!(second.profile_store_action, "unchanged");
        assert_eq!(second.release_channel, "stable");
        assert_eq!(second.release_channel_source, "existing");
        assert_eq!(second.release_channel_action, "unchanged");
        assert_eq!(second.daemon_bootstrap.install_action, "skipped");
        assert_eq!(second.daemon_bootstrap.start_action, "skipped");
    }

    #[test]
    fn functional_build_onboarding_report_applies_release_channel_override() {
        let temp = tempdir().expect("tempdir");
        let mut cli = test_cli();
        apply_workspace_paths(&mut cli, temp.path());
        cli.onboard_profile = "default".to_string();
        cli.onboard_release_channel = Some("beta".to_string());

        let first = build_onboarding_report(&cli, "default", OnboardingMode::NonInteractive)
            .expect("first report");
        assert_eq!(first.release_channel, "beta");
        assert_eq!(first.release_channel_source, "override");
        assert_eq!(first.release_channel_action, "created");

        cli.onboard_release_channel = Some("dev".to_string());
        let second = build_onboarding_report(&cli, "default", OnboardingMode::NonInteractive)
            .expect("second report");
        assert_eq!(second.release_channel, "dev");
        assert_eq!(second.release_channel_source, "override");
        assert_eq!(second.release_channel_action, "updated");
    }

    #[test]
    fn regression_build_onboarding_report_rejects_invalid_release_channel_override() {
        let temp = tempdir().expect("tempdir");
        let mut cli = test_cli();
        apply_workspace_paths(&mut cli, temp.path());
        cli.onboard_release_channel = Some("nightly".to_string());

        let error = build_onboarding_report(&cli, "default", OnboardingMode::NonInteractive)
            .expect_err("invalid release channel should fail");
        assert!(error.to_string().contains("expected stable|beta|dev"));
    }

    #[test]
    fn functional_build_onboarding_report_installs_and_starts_daemon_when_requested() {
        let temp = tempdir().expect("tempdir");
        let mut cli = test_cli();
        apply_workspace_paths(&mut cli, temp.path());
        cli.onboard_install_daemon = true;
        cli.onboard_start_daemon = true;

        let report = build_onboarding_report(&cli, "default", OnboardingMode::NonInteractive)
            .expect("report with daemon bootstrap");

        assert!(report.daemon_bootstrap.requested_install);
        assert!(report.daemon_bootstrap.requested_start);
        assert_eq!(report.daemon_bootstrap.install_action, "installed");
        assert_eq!(report.daemon_bootstrap.start_action, "started");
        assert!(report.daemon_bootstrap.status.installed);
        assert!(report.daemon_bootstrap.status.running);
        assert!(report.daemon_bootstrap.ready);
        assert!(report.daemon_bootstrap.readiness_reason_codes.is_empty());
        assert!(
            PathBuf::from(&report.daemon_bootstrap.status.state_path).exists(),
            "daemon state file should exist"
        );
        assert!(
            PathBuf::from(&report.daemon_bootstrap.status.pid_file_path).exists(),
            "daemon pid marker should exist"
        );
    }

    #[test]
    fn regression_build_onboarding_report_fails_closed_when_daemon_state_dir_is_not_directory() {
        let temp = tempdir().expect("tempdir");
        let mut cli = test_cli();
        apply_workspace_paths(&mut cli, temp.path());
        cli.onboard_install_daemon = true;

        let invalid_state_dir = temp.path().join("daemon-state-file");
        std::fs::write(&invalid_state_dir, "not-a-directory").expect("write invalid state path");
        cli.daemon_state_dir = invalid_state_dir;

        let error = build_onboarding_report(&cli, "default", OnboardingMode::NonInteractive)
            .expect_err("daemon install should fail closed");
        let error_text = error.to_string();
        assert!(error_text.contains("onboarding daemon install failed"));
    }
}
