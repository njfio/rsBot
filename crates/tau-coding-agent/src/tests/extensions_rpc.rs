//! Extension and RPC CLI parsing tests for flag normalization and invalid combinations.

use std::path::PathBuf;

use tau_cli::CliDeploymentWasmRuntimeProfile;

use super::{parse_cli_with_stack, try_parse_cli_with_stack};

#[test]
fn unit_cli_extension_validate_flag_defaults_to_none() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(cli.extension_exec_manifest.is_none());
    assert!(cli.extension_exec_hook.is_none());
    assert!(cli.extension_exec_payload_file.is_none());
    assert!(cli.extension_validate.is_none());
    assert!(!cli.extension_list);
    assert_eq!(cli.extension_list_root, PathBuf::from(".tau/extensions"));
    assert!(cli.extension_show.is_none());
    assert!(!cli.extension_runtime_hooks);
    assert_eq!(cli.extension_runtime_root, PathBuf::from(".tau/extensions"));
    assert!(!cli.tool_builder_enabled);
    assert_eq!(
        cli.tool_builder_output_root,
        PathBuf::from(".tau/generated-tools")
    );
    assert_eq!(
        cli.tool_builder_extension_root,
        PathBuf::from(".tau/extensions/generated")
    );
    assert_eq!(cli.tool_builder_max_attempts, 3);
}

#[test]
fn functional_cli_extension_exec_flags_accept_valid_combo() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--extension-exec-manifest",
        "extensions/issue.json",
        "--extension-exec-hook",
        "run-start",
        "--extension-exec-payload-file",
        "extensions/payload.json",
    ]);
    assert_eq!(
        cli.extension_exec_manifest,
        Some(PathBuf::from("extensions/issue.json"))
    );
    assert_eq!(cli.extension_exec_hook.as_deref(), Some("run-start"));
    assert_eq!(
        cli.extension_exec_payload_file,
        Some(PathBuf::from("extensions/payload.json"))
    );
}

#[test]
fn functional_cli_extension_validate_flag_accepts_path() {
    let cli = parse_cli_with_stack(["tau-rs", "--extension-validate", "extensions/issue.json"]);
    assert_eq!(
        cli.extension_validate,
        Some(PathBuf::from("extensions/issue.json"))
    );
}

#[test]
fn functional_cli_extension_show_flag_accepts_path() {
    let cli = parse_cli_with_stack(["tau-rs", "--extension-show", "extensions/issue.json"]);
    assert_eq!(
        cli.extension_show,
        Some(PathBuf::from("extensions/issue.json"))
    );
}

#[test]
fn functional_cli_extension_list_flag_accepts_root_override() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--extension-list",
        "--extension-list-root",
        "extensions",
    ]);
    assert!(cli.extension_list);
    assert_eq!(cli.extension_list_root, PathBuf::from("extensions"));
}

#[test]
fn functional_cli_extension_runtime_hook_flags_accept_root_override() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--extension-runtime-hooks",
        "--extension-runtime-root",
        "extensions",
    ]);
    assert!(cli.extension_runtime_hooks);
    assert_eq!(cli.extension_runtime_root, PathBuf::from("extensions"));
}

#[test]
fn functional_cli_tool_builder_flags_accept_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--tool-builder-enabled",
        "--tool-builder-output-root",
        ".tau/generated-artifacts",
        "--tool-builder-extension-root",
        ".tau/extensions/generated-runtime",
        "--tool-builder-max-attempts",
        "6",
    ]);
    assert!(cli.tool_builder_enabled);
    assert_eq!(
        cli.tool_builder_output_root,
        PathBuf::from(".tau/generated-artifacts")
    );
    assert_eq!(
        cli.tool_builder_extension_root,
        PathBuf::from(".tau/extensions/generated-runtime")
    );
    assert_eq!(cli.tool_builder_max_attempts, 6);
}

#[test]
fn regression_cli_extension_show_and_validate_conflict() {
    let parse = try_parse_cli_with_stack([
        "tau-rs",
        "--extension-show",
        "extensions/issue.json",
        "--extension-validate",
        "extensions/issue.json",
    ]);
    let error = parse.expect_err("show and validate should conflict");
    assert!(error.to_string().contains("cannot be used with"));
}

#[test]
fn regression_cli_extension_list_and_show_conflict() {
    let parse = try_parse_cli_with_stack([
        "tau-rs",
        "--extension-list",
        "--extension-show",
        "extensions/issue.json",
    ]);
    let error = parse.expect_err("list and show should conflict");
    assert!(error.to_string().contains("cannot be used with"));
}

#[test]
fn regression_cli_extension_exec_requires_hook_and_payload() {
    let parse = try_parse_cli_with_stack([
        "tau-rs",
        "--extension-exec-manifest",
        "extensions/issue.json",
    ]);
    let error = parse.expect_err("exec manifest should require hook and payload");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn regression_cli_extension_runtime_root_requires_runtime_hooks_flag() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--extension-runtime-root", "extensions"]);
    let error = parse.expect_err("runtime root should require runtime hooks flag");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn regression_cli_tool_builder_output_root_requires_enable_flag() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--tool-builder-output-root", "generated"]);
    let error = parse.expect_err("tool builder output root should require enable flag");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn functional_cli_deployment_wasm_package_flags_accept_explicit_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--deployment-wasm-package-module",
        "fixtures/edge.wasm",
        "--deployment-wasm-package-blueprint-id",
        "edge-staging",
        "--deployment-wasm-package-runtime-profile",
        "wasm-wasi",
        "--deployment-wasm-package-output-dir",
        ".tau/deployment/build-output",
        "--deployment-wasm-package-json",
    ]);
    assert_eq!(
        cli.deployment_wasm_package_module,
        Some(PathBuf::from("fixtures/edge.wasm"))
    );
    assert_eq!(
        cli.deployment_wasm_package_blueprint_id,
        "edge-staging".to_string()
    );
    assert_eq!(
        cli.deployment_wasm_package_runtime_profile,
        CliDeploymentWasmRuntimeProfile::WasmWasi
    );
    assert_eq!(
        cli.deployment_wasm_package_output_dir,
        PathBuf::from(".tau/deployment/build-output")
    );
    assert!(cli.deployment_wasm_package_json);
}

#[test]
fn functional_cli_deployment_wasm_package_runtime_profile_accepts_channel_automation_wasi() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--deployment-wasm-package-module",
        "fixtures/edge.wasm",
        "--deployment-wasm-package-runtime-profile",
        "channel-automation-wasi",
    ]);
    assert_eq!(
        cli.deployment_wasm_package_runtime_profile,
        CliDeploymentWasmRuntimeProfile::ChannelAutomationWasi
    );
}

#[test]
fn regression_cli_deployment_wasm_package_json_requires_module_flag() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--deployment-wasm-package-json"]);
    let error = parse.expect_err("package json should require package module");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn regression_cli_deployment_wasm_package_conflicts_with_deployment_runner() {
    let parse = try_parse_cli_with_stack([
        "tau-rs",
        "--deployment-contract-runner",
        "--deployment-wasm-package-module",
        "fixtures/edge.wasm",
    ]);
    let error = parse.expect_err("wasm package should conflict with deployment runner");
    assert!(error.to_string().contains("cannot be used with"));
}

#[test]
fn regression_cli_deployment_wasm_inspect_conflicts_with_package_mode() {
    let parse = try_parse_cli_with_stack([
        "tau-rs",
        "--deployment-wasm-package-module",
        "fixtures/edge.wasm",
        "--deployment-wasm-inspect-manifest",
        "fixtures/edge.manifest.json",
    ]);
    let error = parse.expect_err("inspect mode should conflict with package mode");
    assert!(error.to_string().contains("cannot be used with"));
}

#[test]
fn unit_cli_rpc_flags_default_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.mcp_server);
    assert!(!cli.mcp_client);
    assert!(!cli.mcp_client_inspect);
    assert!(!cli.mcp_client_inspect_json);
    assert!(cli.mcp_external_server_config.is_none());
    assert!(cli.mcp_context_provider.is_empty());
    assert!(!cli.rpc_capabilities);
    assert!(cli.rpc_validate_frame_file.is_none());
    assert!(cli.rpc_dispatch_frame_file.is_none());
    assert!(cli.rpc_dispatch_ndjson_file.is_none());
    assert!(!cli.rpc_serve_ndjson);
}

#[test]
fn regression_cli_mcp_server_conflicts_with_rpc_serve_ndjson() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--mcp-server", "--rpc-serve-ndjson"]);
    let error = parse.expect_err("mcp server and rpc serve ndjson should conflict");
    assert!(error.to_string().contains("cannot be used with"));
}

#[test]
fn regression_cli_mcp_client_conflicts_with_mcp_server() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--mcp-client", "--mcp-server"]);
    let error = parse.expect_err("mcp client and mcp server should conflict");
    assert!(error.to_string().contains("cannot be used with"));
}

#[test]
fn functional_cli_rpc_serve_ndjson_flag_accepts_enablement() {
    let cli = parse_cli_with_stack(["tau-rs", "--rpc-serve-ndjson"]);
    assert!(cli.rpc_serve_ndjson);
}

#[test]
fn regression_cli_rpc_serve_ndjson_conflicts_with_rpc_capabilities() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--rpc-serve-ndjson", "--rpc-capabilities"]);
    let error = parse.expect_err("rpc serve ndjson and rpc capabilities should conflict");
    assert!(error.to_string().contains("cannot be used with"));
}

#[test]
fn regression_cli_rpc_serve_ndjson_conflicts_with_rpc_dispatch_ndjson_file() {
    let parse = try_parse_cli_with_stack([
        "tau-rs",
        "--rpc-serve-ndjson",
        "--rpc-dispatch-ndjson-file",
        "fixtures/rpc.ndjson",
    ]);
    let error = parse.expect_err("rpc serve ndjson and rpc dispatch ndjson should conflict");
    assert!(error.to_string().contains("cannot be used with"));
}
