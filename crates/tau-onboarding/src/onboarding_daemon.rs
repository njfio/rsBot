use anyhow::{Context, Result};
use std::collections::BTreeSet;
use tau_cli::Cli;
use tau_ops::{inspect_tau_daemon, install_tau_daemon, start_tau_daemon, TauDaemonConfig};

use crate::onboarding_report::OnboardingDaemonBootstrapReport;

pub fn run_onboarding_daemon_bootstrap(cli: &Cli) -> Result<OnboardingDaemonBootstrapReport> {
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

#[cfg(test)]
mod tests {
    use super::run_onboarding_daemon_bootstrap;
    use clap::Parser;
    use tau_cli::Cli;
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

    #[test]
    fn functional_run_onboarding_daemon_bootstrap_installs_and_starts_when_requested() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        cli.daemon_state_dir = temp.path().join(".tau/daemon");
        cli.onboard_install_daemon = true;
        cli.onboard_start_daemon = true;

        let report = run_onboarding_daemon_bootstrap(&cli).expect("daemon bootstrap");
        assert!(report.requested_install);
        assert!(report.requested_start);
        assert_eq!(report.install_action, "installed");
        assert_eq!(report.start_action, "started");
        assert!(report.status.installed);
        assert!(report.status.running);
        assert!(report.ready);
    }

    #[test]
    fn integration_run_onboarding_daemon_bootstrap_skips_when_not_requested() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        cli.daemon_state_dir = temp.path().join(".tau/daemon");
        let report = run_onboarding_daemon_bootstrap(&cli).expect("daemon bootstrap");
        assert!(!report.requested_install);
        assert!(!report.requested_start);
        assert_eq!(report.install_action, "skipped");
        assert_eq!(report.start_action, "skipped");
    }

    #[test]
    fn regression_run_onboarding_daemon_bootstrap_fails_closed_on_invalid_state_dir() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        let invalid_state_dir = temp.path().join("daemon-state-file");
        std::fs::write(&invalid_state_dir, "not-a-directory").expect("write invalid state path");
        cli.daemon_state_dir = invalid_state_dir;
        cli.onboard_install_daemon = true;

        let error =
            run_onboarding_daemon_bootstrap(&cli).expect_err("daemon install should fail closed");
        assert!(error
            .to_string()
            .contains("onboarding daemon install failed"));
    }
}
