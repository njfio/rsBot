use anyhow::{Context, Result};
use std::io::{self, Write};
use tau_cli::Cli;
use tau_core::current_unix_timestamp_ms;

use crate::onboarding_daemon::run_onboarding_daemon_bootstrap;
use crate::onboarding_paths::{
    collect_bootstrap_directories, parse_yes_no_response, resolve_tau_root,
};
use crate::onboarding_profile_bootstrap::{
    ensure_directory, ensure_profile_store_entry, resolve_onboarding_profile_name,
};
use crate::onboarding_release_channel::ensure_onboarding_release_channel;
use crate::onboarding_report::{
    build_onboarding_next_steps, collect_executable_checks, render_onboarding_summary,
    resolve_onboarding_report_path, write_onboarding_report, OnboardingReport,
};
use crate::startup_prompt_composition::resolve_startup_identity_report;

const ONBOARDING_REPORT_SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OnboardingMode {
    Interactive,
    NonInteractive,
}

pub fn execute_onboarding_command(cli: &Cli) -> Result<()> {
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
    let identity_composition = resolve_startup_identity_report(cli);
    let next_steps = build_onboarding_next_steps(
        cli,
        &executable_checks,
        &daemon_bootstrap,
        &identity_composition,
    );

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
        identity_composition,
        daemon_bootstrap,
        next_steps,
    })
}

fn onboarding_mode_label(mode: OnboardingMode) -> &'static str {
    match mode {
        OnboardingMode::Interactive => "interactive",
        OnboardingMode::NonInteractive => "non-interactive",
    }
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
    use super::{build_onboarding_report, OnboardingMode, ONBOARDING_REPORT_SCHEMA_VERSION};
    use crate::onboarding_paths::{parse_yes_no_response, resolve_tau_root};
    use clap::Parser;
    use std::path::{Path, PathBuf};
    use tau_cli::Cli;
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
