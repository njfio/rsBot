use super::*;
use tau_onboarding::startup_preflight::{
    execute_startup_preflight as execute_onboarding_startup_preflight, StartupPreflightCallbacks,
};

pub(crate) fn execute_startup_preflight(cli: &Cli) -> Result<bool> {
    let callbacks = StartupPreflightCallbacks {
        execute_onboarding_command,
        execute_multi_channel_send_command: crate::channel_send::execute_multi_channel_send_command,
        execute_multi_channel_channel_lifecycle_command:
            crate::channel_lifecycle::execute_multi_channel_channel_lifecycle_command,
        execute_deployment_wasm_package_command:
            crate::deployment_wasm::execute_deployment_wasm_package_command,
        execute_deployment_wasm_inspect_command:
            crate::deployment_wasm::execute_deployment_wasm_inspect_command,
        execute_project_index_command,
        execute_channel_store_admin_command,
        execute_multi_channel_live_readiness_preflight_command,
        execute_browser_automation_preflight_command,
        execute_extension_exec_command,
        execute_extension_list_command,
        execute_extension_show_command,
        execute_extension_validate_command,
        execute_package_validate_command,
        execute_package_show_command,
        execute_package_install_command,
        execute_package_update_command,
        execute_package_list_command,
        execute_package_remove_command,
        execute_package_rollback_command,
        execute_package_conflicts_command,
        execute_package_activate_command,
        execute_qa_loop_preflight_command,
        execute_mcp_server_command,
        execute_rpc_capabilities_command,
        execute_rpc_validate_frame_command,
        execute_rpc_dispatch_frame_command,
        execute_rpc_dispatch_ndjson_command,
        execute_rpc_serve_ndjson_command,
        execute_events_inspect_command,
        execute_events_validate_command,
        execute_events_simulate_command,
        execute_events_dry_run_command,
        execute_events_template_write_command,
        resolve_secret_from_cli_or_store_id,
        handle_daemon_commands: tau_onboarding::startup_daemon_preflight::handle_daemon_commands,
    };
    execute_onboarding_startup_preflight(cli, &callbacks)
}
