//! Startup preflight guards executed before model/policy resolution and dispatch.
//!
//! Preflight decides whether startup should continue, short-circuit for daemon
//! commands, or stop due to missing prerequisites (for example, non-interactive
//! onboarding constraints). This keeps later phases focused on execution only.

use anyhow::Result;
use std::io::IsTerminal;
use tau_cli::Cli;

use crate::onboarding_wizard::detect_onboarding_first_run_state;

pub type StartupCommandFn = fn(&Cli) -> Result<()>;
pub type ResolveSecretFn = fn(&Cli, Option<&str>, Option<&str>, &str) -> Result<Option<String>>;
pub type HandleDaemonCommandsFn = fn(&Cli) -> Result<bool>;

#[derive(Clone, Copy)]
/// Public struct `StartupPreflightCallbacks` used across Tau components.
pub struct StartupPreflightCallbacks {
    pub execute_onboarding_command: StartupCommandFn,
    pub execute_multi_channel_send_command: StartupCommandFn,
    pub execute_multi_channel_channel_lifecycle_command: StartupCommandFn,
    pub execute_deployment_wasm_package_command: StartupCommandFn,
    pub execute_deployment_wasm_inspect_command: StartupCommandFn,
    pub execute_deployment_wasm_browser_did_init_command: StartupCommandFn,
    pub execute_project_index_command: StartupCommandFn,
    pub execute_channel_store_admin_command: StartupCommandFn,
    pub execute_multi_channel_live_readiness_preflight_command: StartupCommandFn,
    pub execute_browser_automation_preflight_command: StartupCommandFn,
    pub execute_extension_exec_command: StartupCommandFn,
    pub execute_extension_list_command: StartupCommandFn,
    pub execute_extension_show_command: StartupCommandFn,
    pub execute_extension_validate_command: StartupCommandFn,
    pub execute_package_validate_command: StartupCommandFn,
    pub execute_package_show_command: StartupCommandFn,
    pub execute_package_install_command: StartupCommandFn,
    pub execute_package_update_command: StartupCommandFn,
    pub execute_package_list_command: StartupCommandFn,
    pub execute_package_remove_command: StartupCommandFn,
    pub execute_package_rollback_command: StartupCommandFn,
    pub execute_package_conflicts_command: StartupCommandFn,
    pub execute_package_activate_command: StartupCommandFn,
    pub execute_prompt_optimization_control_command: StartupCommandFn,
    pub execute_qa_loop_preflight_command: StartupCommandFn,
    pub execute_mcp_client_inspect_command: StartupCommandFn,
    pub execute_mcp_server_command: StartupCommandFn,
    pub execute_rpc_capabilities_command: StartupCommandFn,
    pub execute_rpc_validate_frame_command: StartupCommandFn,
    pub execute_rpc_dispatch_frame_command: StartupCommandFn,
    pub execute_rpc_dispatch_ndjson_command: StartupCommandFn,
    pub execute_rpc_serve_ndjson_command: StartupCommandFn,
    pub execute_events_inspect_command: StartupCommandFn,
    pub execute_events_validate_command: StartupCommandFn,
    pub execute_events_simulate_command: StartupCommandFn,
    pub execute_events_dry_run_command: StartupCommandFn,
    pub execute_events_template_write_command: StartupCommandFn,
    pub resolve_secret_from_cli_or_store_id: ResolveSecretFn,
    pub handle_daemon_commands: HandleDaemonCommandsFn,
}

struct CallbackStartupPreflightActions<'a> {
    callbacks: &'a StartupPreflightCallbacks,
}

impl tau_startup::StartupPreflightActions for CallbackStartupPreflightActions<'_> {
    fn execute_onboarding_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_onboarding_command)(cli)
    }

    fn execute_multi_channel_send_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_multi_channel_send_command)(cli)
    }

    fn execute_multi_channel_channel_lifecycle_command(&self, cli: &Cli) -> Result<()> {
        (self
            .callbacks
            .execute_multi_channel_channel_lifecycle_command)(cli)
    }

    fn execute_deployment_wasm_package_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_deployment_wasm_package_command)(cli)
    }

    fn execute_deployment_wasm_inspect_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_deployment_wasm_inspect_command)(cli)
    }

    fn execute_deployment_wasm_browser_did_init_command(&self, cli: &Cli) -> Result<()> {
        (self
            .callbacks
            .execute_deployment_wasm_browser_did_init_command)(cli)
    }

    fn execute_project_index_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_project_index_command)(cli)
    }

    fn execute_channel_store_admin_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_channel_store_admin_command)(cli)
    }

    fn execute_multi_channel_live_readiness_preflight_command(&self, cli: &Cli) -> Result<()> {
        (self
            .callbacks
            .execute_multi_channel_live_readiness_preflight_command)(cli)
    }

    fn execute_browser_automation_preflight_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_browser_automation_preflight_command)(cli)
    }

    fn execute_extension_exec_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_extension_exec_command)(cli)
    }

    fn execute_extension_list_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_extension_list_command)(cli)
    }

    fn execute_extension_show_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_extension_show_command)(cli)
    }

    fn execute_extension_validate_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_extension_validate_command)(cli)
    }

    fn execute_package_validate_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_package_validate_command)(cli)
    }

    fn execute_package_show_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_package_show_command)(cli)
    }

    fn execute_package_install_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_package_install_command)(cli)
    }

    fn execute_package_update_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_package_update_command)(cli)
    }

    fn execute_package_list_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_package_list_command)(cli)
    }

    fn execute_package_remove_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_package_remove_command)(cli)
    }

    fn execute_package_rollback_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_package_rollback_command)(cli)
    }

    fn execute_package_conflicts_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_package_conflicts_command)(cli)
    }

    fn execute_package_activate_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_package_activate_command)(cli)
    }

    fn execute_prompt_optimization_control_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_prompt_optimization_control_command)(cli)
    }

    fn execute_qa_loop_preflight_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_qa_loop_preflight_command)(cli)
    }

    fn execute_mcp_client_inspect_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_mcp_client_inspect_command)(cli)
    }

    fn execute_mcp_server_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_mcp_server_command)(cli)
    }

    fn execute_rpc_capabilities_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_rpc_capabilities_command)(cli)
    }

    fn execute_rpc_validate_frame_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_rpc_validate_frame_command)(cli)
    }

    fn execute_rpc_dispatch_frame_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_rpc_dispatch_frame_command)(cli)
    }

    fn execute_rpc_dispatch_ndjson_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_rpc_dispatch_ndjson_command)(cli)
    }

    fn execute_rpc_serve_ndjson_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_rpc_serve_ndjson_command)(cli)
    }

    fn execute_events_inspect_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_events_inspect_command)(cli)
    }

    fn execute_events_validate_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_events_validate_command)(cli)
    }

    fn execute_events_simulate_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_events_simulate_command)(cli)
    }

    fn execute_events_dry_run_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_events_dry_run_command)(cli)
    }

    fn execute_events_template_write_command(&self, cli: &Cli) -> Result<()> {
        (self.callbacks.execute_events_template_write_command)(cli)
    }

    fn resolve_secret_from_cli_or_store_id(
        &self,
        cli: &Cli,
        direct_secret: Option<&str>,
        secret_id: Option<&str>,
        secret_id_flag: &str,
    ) -> Result<Option<String>> {
        (self.callbacks.resolve_secret_from_cli_or_store_id)(
            cli,
            direct_secret,
            secret_id,
            secret_id_flag,
        )
    }

    fn handle_daemon_commands(&self, cli: &Cli) -> Result<bool> {
        (self.callbacks.handle_daemon_commands)(cli)
    }
}

pub fn execute_startup_preflight(cli: &Cli, callbacks: &StartupPreflightCallbacks) -> Result<bool> {
    execute_startup_preflight_with_detection(
        cli,
        callbacks,
        std::env::args_os().count(),
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
        onboarding_auto_enabled_from_env(),
    )
}

fn execute_startup_preflight_with_detection(
    cli: &Cli,
    callbacks: &StartupPreflightCallbacks,
    arg_count: usize,
    stdin_is_tty: bool,
    stdout_is_tty: bool,
    auto_enabled: bool,
) -> Result<bool> {
    let actions = CallbackStartupPreflightActions { callbacks };
    let handled = tau_startup::execute_startup_preflight(cli, &actions)?;
    if handled {
        return Ok(true);
    }

    let first_run = detect_onboarding_first_run_state(cli);
    if should_auto_run_onboarding(
        cli,
        first_run.is_first_run,
        arg_count,
        stdin_is_tty,
        stdout_is_tty,
        auto_enabled,
    ) {
        (callbacks.execute_onboarding_command)(cli)?;
        return Ok(true);
    }
    Ok(false)
}

fn onboarding_auto_enabled_from_env() -> bool {
    match std::env::var("TAU_ONBOARD_AUTO") {
        Ok(raw) => {
            let normalized = raw.trim().to_ascii_lowercase();
            !matches!(normalized.as_str(), "0" | "false" | "no" | "off")
        }
        Err(_) => true,
    }
}

fn should_auto_run_onboarding(
    cli: &Cli,
    first_run_detected: bool,
    arg_count: usize,
    stdin_is_tty: bool,
    stdout_is_tty: bool,
    auto_enabled: bool,
) -> bool {
    if !auto_enabled {
        return false;
    }
    if cli.onboard || cli.onboard_non_interactive {
        return false;
    }
    if arg_count > 1 {
        return false;
    }
    if !stdin_is_tty || !stdout_is_tty {
        return false;
    }
    first_run_detected
}

#[cfg(test)]
mod tests {
    use super::{
        execute_startup_preflight_with_detection, should_auto_run_onboarding,
        StartupPreflightCallbacks,
    };
    use anyhow::{anyhow, Result};
    use clap::Parser;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tau_cli::Cli;
    use tempfile::tempdir;

    static ONBOARD_CALLS: AtomicUsize = AtomicUsize::new(0);
    static AUTO_ONBOARD_CALLS: AtomicUsize = AtomicUsize::new(0);
    static DAEMON_CALLS: AtomicUsize = AtomicUsize::new(0);

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

    fn execute_without_auto(cli: &Cli, callbacks: &StartupPreflightCallbacks) -> Result<bool> {
        execute_startup_preflight_with_detection(cli, callbacks, 2, true, true, true)
    }

    fn noop_command(_cli: &Cli) -> Result<()> {
        Ok(())
    }

    fn noop_secret(
        _cli: &Cli,
        _direct_secret: Option<&str>,
        _secret_id: Option<&str>,
        _secret_id_flag: &str,
    ) -> Result<Option<String>> {
        Ok(None)
    }

    fn daemon_false(_cli: &Cli) -> Result<bool> {
        Ok(false)
    }

    fn onboarding_counter(_cli: &Cli) -> Result<()> {
        ONBOARD_CALLS.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn auto_onboarding_counter(_cli: &Cli) -> Result<()> {
        AUTO_ONBOARD_CALLS.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn onboarding_error(_cli: &Cli) -> Result<()> {
        Err(anyhow!("onboarding callback failure"))
    }

    fn daemon_true_counter(_cli: &Cli) -> Result<bool> {
        DAEMON_CALLS.fetch_add(1, Ordering::Relaxed);
        Ok(true)
    }

    fn default_callbacks() -> StartupPreflightCallbacks {
        StartupPreflightCallbacks {
            execute_onboarding_command: noop_command,
            execute_multi_channel_send_command: noop_command,
            execute_multi_channel_channel_lifecycle_command: noop_command,
            execute_deployment_wasm_package_command: noop_command,
            execute_deployment_wasm_inspect_command: noop_command,
            execute_deployment_wasm_browser_did_init_command: noop_command,
            execute_project_index_command: noop_command,
            execute_channel_store_admin_command: noop_command,
            execute_multi_channel_live_readiness_preflight_command: noop_command,
            execute_browser_automation_preflight_command: noop_command,
            execute_extension_exec_command: noop_command,
            execute_extension_list_command: noop_command,
            execute_extension_show_command: noop_command,
            execute_extension_validate_command: noop_command,
            execute_package_validate_command: noop_command,
            execute_package_show_command: noop_command,
            execute_package_install_command: noop_command,
            execute_package_update_command: noop_command,
            execute_package_list_command: noop_command,
            execute_package_remove_command: noop_command,
            execute_package_rollback_command: noop_command,
            execute_package_conflicts_command: noop_command,
            execute_package_activate_command: noop_command,
            execute_prompt_optimization_control_command: noop_command,
            execute_qa_loop_preflight_command: noop_command,
            execute_mcp_client_inspect_command: noop_command,
            execute_mcp_server_command: noop_command,
            execute_rpc_capabilities_command: noop_command,
            execute_rpc_validate_frame_command: noop_command,
            execute_rpc_dispatch_frame_command: noop_command,
            execute_rpc_dispatch_ndjson_command: noop_command,
            execute_rpc_serve_ndjson_command: noop_command,
            execute_events_inspect_command: noop_command,
            execute_events_validate_command: noop_command,
            execute_events_simulate_command: noop_command,
            execute_events_dry_run_command: noop_command,
            execute_events_template_write_command: noop_command,
            resolve_secret_from_cli_or_store_id: noop_secret,
            handle_daemon_commands: daemon_false,
        }
    }

    #[test]
    fn unit_execute_startup_preflight_onboard_calls_callback() {
        ONBOARD_CALLS.store(0, Ordering::Relaxed);
        let mut callbacks = default_callbacks();
        callbacks.execute_onboarding_command = onboarding_counter;
        let mut cli = parse_cli_with_stack();
        cli.onboard = true;

        let handled =
            execute_without_auto(&cli, &callbacks).expect("startup preflight should succeed");
        assert!(handled);
        assert_eq!(ONBOARD_CALLS.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn functional_execute_startup_preflight_delegates_to_daemon_handler() {
        DAEMON_CALLS.store(0, Ordering::Relaxed);
        let mut callbacks = default_callbacks();
        callbacks.handle_daemon_commands = daemon_true_counter;
        let cli = parse_cli_with_stack();

        let handled =
            execute_without_auto(&cli, &callbacks).expect("startup preflight should succeed");
        assert!(handled);
        assert_eq!(DAEMON_CALLS.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn integration_execute_startup_preflight_returns_false_when_no_mode_matches() {
        let callbacks = default_callbacks();
        let cli = parse_cli_with_stack();

        let handled =
            execute_without_auto(&cli, &callbacks).expect("startup preflight should succeed");
        assert!(!handled);
    }

    #[test]
    fn regression_execute_startup_preflight_propagates_callback_errors() {
        let mut callbacks = default_callbacks();
        callbacks.execute_onboarding_command = onboarding_error;
        let mut cli = parse_cli_with_stack();
        cli.onboard = true;

        let error = execute_without_auto(&cli, &callbacks)
            .expect_err("onboarding callback failure should propagate");
        assert!(error.to_string().contains("onboarding callback failure"));
    }

    #[test]
    fn functional_execute_startup_preflight_auto_runs_onboarding_on_first_run_default_invocation() {
        AUTO_ONBOARD_CALLS.store(0, Ordering::Relaxed);
        let mut callbacks = default_callbacks();
        callbacks.execute_onboarding_command = auto_onboarding_counter;
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        apply_workspace_paths(&mut cli, temp.path());

        let handled =
            execute_startup_preflight_with_detection(&cli, &callbacks, 1, true, true, true)
                .expect("startup preflight should succeed");
        assert!(handled);
        assert_eq!(AUTO_ONBOARD_CALLS.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn regression_execute_startup_preflight_auto_onboarding_respects_disable_and_tty_guards() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        apply_workspace_paths(&mut cli, temp.path());
        let callbacks = default_callbacks();

        let handled_disabled =
            execute_startup_preflight_with_detection(&cli, &callbacks, 1, true, true, false)
                .expect("startup preflight should succeed");
        assert!(!handled_disabled);

        let handled_non_tty =
            execute_startup_preflight_with_detection(&cli, &callbacks, 1, false, true, true)
                .expect("startup preflight should succeed");
        assert!(!handled_non_tty);
    }

    #[test]
    fn unit_should_auto_run_onboarding_requires_first_run_and_default_invocation() {
        let mut cli = parse_cli_with_stack();
        assert!(should_auto_run_onboarding(&cli, true, 1, true, true, true));
        assert!(!should_auto_run_onboarding(
            &cli, false, 1, true, true, true
        ));
        assert!(!should_auto_run_onboarding(&cli, true, 2, true, true, true));
        cli.onboard = true;
        assert!(!should_auto_run_onboarding(&cli, true, 1, true, true, true));
    }
}
