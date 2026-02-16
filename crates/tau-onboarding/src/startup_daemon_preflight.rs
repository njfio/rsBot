//! Preflight handler for daemon-only startup commands.
//!
//! This phase runs before core startup dispatch: if daemon-mode flags are present
//! it validates CLI input, executes daemon operations, and short-circuits normal
//! agent startup. Errors are contextualized per daemon action for diagnostics.

use anyhow::{Context, Result};
use tau_cli::{validate_daemon_cli, Cli};
use tau_ops::{
    inspect_tau_daemon, install_tau_daemon, render_tau_daemon_status_report, start_tau_daemon,
    stop_tau_daemon, tau_daemon_mode_requested, uninstall_tau_daemon, TauDaemonConfig,
};

/// Handle daemon CLI commands during startup preflight and short-circuit when executed.
pub fn handle_daemon_commands(cli: &Cli) -> Result<bool> {
    if !tau_daemon_mode_requested(cli) {
        return Ok(false);
    }

    validate_daemon_cli(cli)?;
    let config = TauDaemonConfig {
        state_dir: cli.daemon_state_dir.clone(),
        profile: cli.daemon_profile,
    };

    if cli.daemon_install {
        let report = install_tau_daemon(&config)?;
        println!("{}", render_tau_daemon_status_report(&report));
        return Ok(true);
    }
    if cli.daemon_uninstall {
        let report = uninstall_tau_daemon(&config)?;
        println!("{}", render_tau_daemon_status_report(&report));
        return Ok(true);
    }
    if cli.daemon_start {
        let report = start_tau_daemon(&config)?;
        println!("{}", render_tau_daemon_status_report(&report));
        return Ok(true);
    }
    if cli.daemon_stop {
        let report = stop_tau_daemon(&config, cli.daemon_stop_reason.as_deref())?;
        println!("{}", render_tau_daemon_status_report(&report));
        return Ok(true);
    }
    if cli.daemon_status {
        let report = inspect_tau_daemon(&config)?;
        if cli.daemon_status_json {
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .context("failed to render daemon status json")?
            );
        } else {
            println!("{}", render_tau_daemon_status_report(&report));
        }
        return Ok(true);
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::handle_daemon_commands;
    use clap::Parser;
    use tau_cli::Cli;
    use tempfile::tempdir;

    fn parse_cli_with_stack(args: &[&str]) -> Cli {
        std::thread::Builder::new()
            .name("tau-cli-parse".to_string())
            .stack_size(16 * 1024 * 1024)
            .spawn({
                let args = args
                    .iter()
                    .copied()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>();
                move || Cli::parse_from(args)
            })
            .expect("spawn cli parse thread")
            .join()
            .expect("join cli parse thread")
    }

    #[test]
    fn functional_handle_daemon_commands_runs_status_mode() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack(&["tau-rs", "--daemon-status"]);
        cli.daemon_state_dir = temp.path().join(".tau/daemon");
        let handled = handle_daemon_commands(&cli).expect("daemon status should succeed");
        assert!(handled);
    }

    #[test]
    fn integration_handle_daemon_commands_returns_false_when_not_requested() {
        let cli = parse_cli_with_stack(&["tau-rs"]);
        let handled = handle_daemon_commands(&cli).expect("daemon noop should succeed");
        assert!(!handled);
    }

    #[test]
    fn regression_handle_daemon_commands_fails_closed_on_prompt_conflict() {
        let cli = parse_cli_with_stack(&["tau-rs", "--daemon-status", "--prompt", "conflict"]);
        let error = handle_daemon_commands(&cli).expect_err("prompt conflict should fail");
        assert!(error.to_string().contains("--prompt"));
    }
}
