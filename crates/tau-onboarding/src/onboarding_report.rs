use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tau_cli::{Cli, CliProviderAuthMode};
use tau_core::{current_unix_timestamp_ms, write_text_atomic};
use tau_ops::TauDaemonStatusReport;
use tau_provider::is_executable_available;

use crate::onboarding_paths::resolve_tau_root;
use crate::startup_prompt_composition::StartupIdentityCompositionReport;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `OnboardingExecutableCheck` used across Tau components.
pub struct OnboardingExecutableCheck {
    pub integration: String,
    pub executable: String,
    pub available: bool,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `OnboardingDaemonBootstrapReport` used across Tau components.
pub struct OnboardingDaemonBootstrapReport {
    pub requested_install: bool,
    pub requested_start: bool,
    pub install_action: String,
    pub start_action: String,
    pub ready: bool,
    pub readiness_reason_codes: Vec<String>,
    pub status: TauDaemonStatusReport,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `OnboardingReport` used across Tau components.
pub struct OnboardingReport {
    pub schema_version: u32,
    pub generated_at_ms: u64,
    pub mode: String,
    pub tau_root: String,
    pub profile_name: String,
    pub profile_store_path: String,
    pub profile_store_action: String,
    pub release_channel_path: String,
    pub release_channel: String,
    pub release_channel_source: String,
    pub release_channel_action: String,
    pub directories_created: Vec<String>,
    pub directories_existing: Vec<String>,
    pub files_created: Vec<String>,
    pub files_existing: Vec<String>,
    pub executable_checks: Vec<OnboardingExecutableCheck>,
    pub identity_composition: StartupIdentityCompositionReport,
    pub daemon_bootstrap: OnboardingDaemonBootstrapReport,
    pub next_steps: Vec<String>,
}

pub fn collect_executable_checks(cli: &Cli) -> Vec<OnboardingExecutableCheck> {
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

pub fn build_onboarding_next_steps(
    cli: &Cli,
    executable_checks: &[OnboardingExecutableCheck],
    daemon_bootstrap: &OnboardingDaemonBootstrapReport,
    identity_composition: &StartupIdentityCompositionReport,
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
    if identity_composition.missing_count > 0 {
        next_steps.push(format!(
            "Create identity files under {} (SOUL.md, AGENTS.md, USER.md) to customize startup composition.",
            identity_composition.tau_root
        ));
    }
    next_steps.push("/auth status".to_string());
    next_steps.push(format!(
        "cargo run -p tau-coding-agent -- --model {}",
        cli.model
    ));
    next_steps
}

pub fn resolve_onboarding_report_path(cli: &Cli) -> Result<PathBuf> {
    let tau_root = resolve_tau_root(cli);
    let reports_dir = tau_root.join("reports");
    let report_name = format!("onboarding-{}.json", current_unix_timestamp_ms());
    Ok(reports_dir.join(report_name))
}

pub fn write_onboarding_report(report: &OnboardingReport, report_path: PathBuf) -> Result<PathBuf> {
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

pub fn render_onboarding_summary(report: &OnboardingReport, report_path: &Path) -> String {
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
        format!(
            "identity: loaded={} missing={} skipped={} tau_root={}",
            report.identity_composition.loaded_count,
            report.identity_composition.missing_count,
            report.identity_composition.skipped_count,
            report.identity_composition.tau_root
        ),
    ];
    for reason in &report.daemon_bootstrap.readiness_reason_codes {
        lines.push(format!("daemon_reason: {reason}"));
    }
    for identity in &report.identity_composition.entries {
        lines.push(format!(
            "identity_file: key={} file={} status={} code={} path={}",
            identity.key,
            identity.file_name,
            identity.status.as_str(),
            identity.reason_code,
            identity.path
        ));
    }
    for next_step in &report.next_steps {
        lines.push(format!("next: {next_step}"));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::{
        build_onboarding_next_steps, collect_executable_checks, render_onboarding_summary,
        resolve_onboarding_report_path, write_onboarding_report, OnboardingDaemonBootstrapReport,
        OnboardingReport,
    };
    use crate::startup_prompt_composition::{
        StartupIdentityCompositionReport, StartupIdentityFileReportEntry, StartupIdentityFileStatus,
    };
    use clap::Parser;
    use std::path::Path;
    use tau_cli::Cli;
    use tau_ops::TauDaemonStatusReport;
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
        cli.daemon_state_dir = tau_root.join("daemon");
    }

    fn sample_daemon_status() -> TauDaemonStatusReport {
        TauDaemonStatusReport {
            schema_version: 1,
            state_path: ".tau/daemon/state.json".to_string(),
            installed: true,
            running: false,
            profile: "launchd".to_string(),
            service_label: "io.tau.coding-agent".to_string(),
            service_file_path: ".tau/daemon/tau.service".to_string(),
            service_file_exists: true,
            pid_file_path: ".tau/daemon/daemon.pid".to_string(),
            pid_file_exists: false,
            pid: None,
            host_os: "macos".to_string(),
            profile_supported_on_host: true,
            executable_path: "/tmp/tau".to_string(),
            executable_exists: true,
            state_dir_exists: true,
            state_dir_writable: true,
            last_install_unix_ms: Some(1),
            last_start_unix_ms: Some(2),
            last_stop_unix_ms: Some(3),
            last_stop_reason: Some("test".to_string()),
            start_attempts: 1,
            stop_attempts: 1,
            diagnostics: vec!["daemon_not_running".to_string()],
        }
    }

    fn sample_report() -> OnboardingReport {
        OnboardingReport {
            schema_version: 3,
            generated_at_ms: 123,
            mode: "non-interactive".to_string(),
            tau_root: ".tau".to_string(),
            profile_name: "default".to_string(),
            profile_store_path: ".tau/profiles.json".to_string(),
            profile_store_action: "created".to_string(),
            release_channel_path: ".tau/release-channel.json".to_string(),
            release_channel: "stable".to_string(),
            release_channel_source: "default".to_string(),
            release_channel_action: "created".to_string(),
            directories_created: vec![".tau".to_string()],
            directories_existing: vec![],
            files_created: vec![".tau/profiles.json".to_string()],
            files_existing: vec![],
            executable_checks: vec![],
            identity_composition: StartupIdentityCompositionReport {
                schema_version: 1,
                tau_root: ".tau".to_string(),
                loaded_count: 1,
                missing_count: 2,
                skipped_count: 0,
                entries: vec![
                    StartupIdentityFileReportEntry {
                        key: "soul".to_string(),
                        file_name: "SOUL.md".to_string(),
                        path: ".tau/SOUL.md".to_string(),
                        status: StartupIdentityFileStatus::Loaded,
                        reason_code: "identity_file_loaded".to_string(),
                        bytes: 12,
                    },
                    StartupIdentityFileReportEntry {
                        key: "agents".to_string(),
                        file_name: "AGENTS.md".to_string(),
                        path: ".tau/AGENTS.md".to_string(),
                        status: StartupIdentityFileStatus::Missing,
                        reason_code: "identity_file_missing".to_string(),
                        bytes: 0,
                    },
                    StartupIdentityFileReportEntry {
                        key: "user".to_string(),
                        file_name: "USER.md".to_string(),
                        path: ".tau/USER.md".to_string(),
                        status: StartupIdentityFileStatus::Missing,
                        reason_code: "identity_file_missing".to_string(),
                        bytes: 0,
                    },
                ],
            },
            daemon_bootstrap: OnboardingDaemonBootstrapReport {
                requested_install: false,
                requested_start: true,
                install_action: "skipped".to_string(),
                start_action: "started".to_string(),
                ready: false,
                readiness_reason_codes: vec!["daemon_start_expected_running".to_string()],
                status: sample_daemon_status(),
            },
            next_steps: vec!["/auth status".to_string()],
        }
    }

    #[test]
    fn unit_collect_executable_checks_marks_required_auth_workflows() {
        let mut cli = parse_cli_with_stack();
        cli.openai_codex_backend = true;
        cli.openai_auth_mode = tau_cli::CliProviderAuthMode::OauthToken;
        cli.anthropic_claude_backend = true;
        cli.anthropic_auth_mode = tau_cli::CliProviderAuthMode::ApiKey;
        cli.google_gemini_backend = true;
        cli.google_auth_mode = tau_cli::CliProviderAuthMode::Adc;

        let checks = collect_executable_checks(&cli);
        let openai = checks
            .iter()
            .find(|entry| entry.integration == "openai-codex")
            .expect("openai check");
        let anthropic = checks
            .iter()
            .find(|entry| entry.integration == "anthropic-claude")
            .expect("anthropic check");
        let gcloud = checks
            .iter()
            .find(|entry| entry.integration == "google-gcloud")
            .expect("gcloud check");

        assert!(openai.required);
        assert!(!anthropic.required);
        assert!(gcloud.required);
    }

    #[test]
    fn functional_report_path_and_write_round_trip_persists_payload() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        apply_workspace_paths(&mut cli, temp.path());
        let path = resolve_onboarding_report_path(&cli).expect("resolve report path");
        let report = sample_report();

        let written = write_onboarding_report(&report, path).expect("write report");
        let raw = std::fs::read_to_string(&written).expect("read written report");
        let parsed: OnboardingReport = serde_json::from_str(&raw).expect("parse written report");
        assert_eq!(parsed.profile_name, "default");
        assert_eq!(
            written.extension().and_then(|ext| ext.to_str()),
            Some("json")
        );
    }

    #[test]
    fn regression_render_onboarding_summary_includes_reason_codes_and_next_steps() {
        let report = sample_report();
        let summary = render_onboarding_summary(&report, Path::new(".tau/reports/test.json"));
        assert!(summary.contains("daemon_reason: daemon_start_expected_running"));
        assert!(summary.contains("identity: loaded=1 missing=2 skipped=0 tau_root=.tau"));
        assert!(summary.contains(
            "identity_file: key=soul file=SOUL.md status=loaded code=identity_file_loaded path=.tau/SOUL.md"
        ));
        assert!(summary.contains("next: /auth status"));
    }

    #[test]
    fn integration_build_onboarding_next_steps_adds_daemon_recovery_actions() {
        let mut cli = parse_cli_with_stack();
        cli.daemon_state_dir = Path::new(".tau/daemon").to_path_buf();
        cli.model = "openai/gpt-4o-mini".to_string();
        let checks = vec![];
        let mut status = sample_daemon_status();
        status.installed = false;
        status.running = false;
        let daemon_bootstrap = OnboardingDaemonBootstrapReport {
            requested_install: true,
            requested_start: true,
            install_action: "installed".to_string(),
            start_action: "started".to_string(),
            ready: false,
            readiness_reason_codes: vec!["daemon_start_expected_running".to_string()],
            status,
        };

        let next_steps = build_onboarding_next_steps(
            &cli,
            &checks,
            &daemon_bootstrap,
            &sample_report().identity_composition,
        );
        let joined = next_steps.join("\n");
        assert!(joined.contains("--daemon-install"));
        assert!(joined.contains("--daemon-start"));
        assert!(joined.contains("--daemon-status --daemon-status-json"));
        assert!(joined.contains("--model openai/gpt-4o-mini"));
    }
}
