use anyhow::Result;
use tau_cli::Cli;

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
    let actions = CallbackStartupPreflightActions { callbacks };
    tau_startup::execute_startup_preflight(cli, &actions)
}

#[cfg(test)]
mod tests {
    use super::{execute_startup_preflight, StartupPreflightCallbacks};
    use anyhow::{anyhow, Result};
    use clap::Parser;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tau_cli::Cli;

    static ONBOARD_CALLS: AtomicUsize = AtomicUsize::new(0);
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
            execute_startup_preflight(&cli, &callbacks).expect("startup preflight should succeed");
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
            execute_startup_preflight(&cli, &callbacks).expect("startup preflight should succeed");
        assert!(handled);
        assert_eq!(DAEMON_CALLS.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn integration_execute_startup_preflight_returns_false_when_no_mode_matches() {
        let callbacks = default_callbacks();
        let cli = parse_cli_with_stack();

        let handled =
            execute_startup_preflight(&cli, &callbacks).expect("startup preflight should succeed");
        assert!(!handled);
    }

    #[test]
    fn regression_execute_startup_preflight_propagates_callback_errors() {
        let mut callbacks = default_callbacks();
        callbacks.execute_onboarding_command = onboarding_error;
        let mut cli = parse_cli_with_stack();
        cli.onboard = true;

        let error = execute_startup_preflight(&cli, &callbacks)
            .expect_err("onboarding callback failure should propagate");
        assert!(error.to_string().contains("onboarding callback failure"));
    }
}
