use anyhow::{Context, Result};
use tau_cli::Cli;

/// Re-exports for deployment WASM package/inspect command integration.
pub use crate::deployment_wasm::{
    inspect_deployment_wasm_deliverable, package_deployment_wasm_artifact,
    render_deployment_wasm_inspect_report, render_deployment_wasm_package_report,
    DeploymentWasmPackageConfig,
};
/// Re-exports for browser DID initialization command integration.
pub use crate::deployment_wasm_identity::{
    initialize_deployment_wasm_browser_did, render_deployment_wasm_browser_did_report,
    DeploymentWasmBrowserDidConfig,
};

/// Execute WASM package command when --deployment-wasm-package-module is set.
pub fn execute_deployment_wasm_package_command(cli: &Cli) -> Result<()> {
    let Some(module_path) = cli.deployment_wasm_package_module.clone() else {
        return Ok(());
    };
    let report = package_deployment_wasm_artifact(&DeploymentWasmPackageConfig {
        module_path,
        blueprint_id: cli.deployment_wasm_package_blueprint_id.clone(),
        runtime_profile: cli
            .deployment_wasm_package_runtime_profile
            .as_str()
            .to_string(),
        output_dir: cli.deployment_wasm_package_output_dir.clone(),
        state_dir: cli.deployment_state_dir.clone(),
    })?;
    if cli.deployment_wasm_package_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to render deployment wasm package report json")?
        );
    } else {
        println!("{}", render_deployment_wasm_package_report(&report));
    }
    Ok(())
}

/// Execute WASM inspect command when --deployment-wasm-inspect-manifest is set.
pub fn execute_deployment_wasm_inspect_command(cli: &Cli) -> Result<()> {
    let Some(manifest_path) = cli.deployment_wasm_inspect_manifest.clone() else {
        return Ok(());
    };
    let report = inspect_deployment_wasm_deliverable(&manifest_path)?;
    if cli.deployment_wasm_inspect_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to render deployment wasm inspect report json")?
        );
    } else {
        println!("{}", render_deployment_wasm_inspect_report(&report));
    }
    Ok(())
}

/// Execute browser DID initialization command for deployment WASM runtime.
pub fn execute_deployment_wasm_browser_did_init_command(cli: &Cli) -> Result<()> {
    if !cli.deployment_wasm_browser_did_init {
        return Ok(());
    }

    let report = initialize_deployment_wasm_browser_did(&DeploymentWasmBrowserDidConfig {
        method: match cli.deployment_wasm_browser_did_method {
            tau_cli::CliDeploymentWasmBrowserDidMethod::Key => kamn_sdk::DidMethod::Key,
            tau_cli::CliDeploymentWasmBrowserDidMethod::Web => kamn_sdk::DidMethod::Web,
        },
        network: cli.deployment_wasm_browser_did_network.trim().to_string(),
        subject: cli.deployment_wasm_browser_did_subject.trim().to_string(),
        entropy: cli.deployment_wasm_browser_did_entropy.trim().to_string(),
        output_path: Some(cli.deployment_wasm_browser_did_output.clone()),
    })?;

    if cli.deployment_wasm_browser_did_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to render deployment wasm browser did report json")?
        );
    } else {
        println!("{}", render_deployment_wasm_browser_did_report(&report));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        execute_deployment_wasm_browser_did_init_command, execute_deployment_wasm_inspect_command,
        execute_deployment_wasm_package_command, package_deployment_wasm_artifact,
        DeploymentWasmPackageConfig,
    };
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

    fn write_test_wasm_module(path: &Path) {
        let bytes = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        std::fs::write(path, bytes).expect("write wasm module");
    }

    #[test]
    fn unit_execute_deployment_wasm_package_command_noops_without_module_flag() {
        let cli = parse_cli_with_stack();
        execute_deployment_wasm_package_command(&cli).expect("package command should noop");
    }

    #[test]
    fn unit_execute_deployment_wasm_browser_did_init_noops_without_flag() {
        let cli = parse_cli_with_stack();
        execute_deployment_wasm_browser_did_init_command(&cli)
            .expect("browser did init should noop");
    }

    #[test]
    fn functional_execute_deployment_wasm_package_command_creates_manifest_and_state_files() {
        let temp = tempdir().expect("tempdir");
        let module_path = temp.path().join("edge.wasm");
        let output_dir = temp.path().join("out");
        let state_dir = temp.path().join(".tau/deployment");
        write_test_wasm_module(&module_path);

        let mut cli = parse_cli_with_stack();
        cli.deployment_wasm_package_module = Some(module_path);
        cli.deployment_wasm_package_output_dir = output_dir.clone();
        cli.deployment_state_dir = state_dir.clone();
        cli.deployment_wasm_package_json = true;

        execute_deployment_wasm_package_command(&cli).expect("package command should succeed");
        assert!(std::fs::read_dir(&output_dir)
            .expect("read output dir")
            .next()
            .is_some());
        assert!(
            state_dir.exists(),
            "deployment state directory should exist"
        );
    }

    #[test]
    fn integration_execute_deployment_wasm_inspect_command_succeeds_for_packaged_manifest() {
        let temp = tempdir().expect("tempdir");
        let module_path = temp.path().join("edge.wasm");
        write_test_wasm_module(&module_path);

        let package_report = package_deployment_wasm_artifact(&DeploymentWasmPackageConfig {
            module_path,
            blueprint_id: "edge-wasm".to_string(),
            runtime_profile: "wasm_wasi".to_string(),
            output_dir: temp.path().join("out"),
            state_dir: temp.path().join(".tau/deployment"),
        })
        .expect("package wasm");

        let mut cli = parse_cli_with_stack();
        cli.deployment_wasm_inspect_manifest = Some(package_report.manifest_path.into());
        cli.deployment_wasm_inspect_json = true;

        execute_deployment_wasm_inspect_command(&cli).expect("inspect command should succeed");
    }

    #[test]
    fn regression_execute_deployment_wasm_inspect_command_fails_for_missing_manifest() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        cli.deployment_wasm_inspect_manifest = Some(temp.path().join("missing.manifest.json"));

        let error = execute_deployment_wasm_inspect_command(&cli)
            .expect_err("inspect command should fail for missing manifest");
        assert!(error
            .to_string()
            .contains("failed to read deployment wasm manifest"));
    }

    #[test]
    fn functional_execute_deployment_wasm_browser_did_init_writes_report() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        cli.deployment_wasm_browser_did_init = true;
        cli.deployment_wasm_browser_did_output = temp.path().join("browser-did.json");
        cli.deployment_wasm_browser_did_subject = "edge-agent".to_string();
        cli.deployment_wasm_browser_did_entropy = "seed-42".to_string();
        cli.deployment_wasm_browser_did_json = true;

        execute_deployment_wasm_browser_did_init_command(&cli)
            .expect("browser did init should succeed");

        let payload = std::fs::read_to_string(&cli.deployment_wasm_browser_did_output)
            .expect("read browser did report");
        assert!(payload.contains("\"did\": \"did:key:"));
    }
}
