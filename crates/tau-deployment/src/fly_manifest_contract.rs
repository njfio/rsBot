use anyhow::{bail, Context, Result};
use std::path::PathBuf;

const FLY_MANIFEST_RELATIVE_PATH: &str = "fly.toml";
const DEPLOYMENT_RUNBOOK_RELATIVE_PATH: &str = "docs/guides/deployment-ops.md";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Public `fn` `load_repo_fly_manifest` in `tau-deployment`.
pub fn load_repo_fly_manifest() -> Result<String> {
    let path = repo_root().join(FLY_MANIFEST_RELATIVE_PATH);
    std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read fly manifest {}", path.display()))
}

/// Public `fn` `validate_fly_manifest_contract` in `tau-deployment`.
pub fn validate_fly_manifest_contract(raw: &str) -> Result<()> {
    require_fragment(raw, "[build]")?;
    require_fragment(raw, "[env]")?;
    require_fragment(raw, "[http_service]")?;
    require_fragment(raw, "[[http_service.checks]]")?;
    require_fragment(raw, "dockerfile = \"Dockerfile\"")?;
    require_fragment(raw, "TAU_TRANSPORT_MODE = \"gateway\"")?;
    require_fragment(raw, "TAU_GATEWAY_OPENRESPONSES_SERVER = \"true\"")?;
    require_fragment(raw, "TAU_GATEWAY_OPENRESPONSES_BIND = \"0.0.0.0:8080\"")?;
    require_fragment(raw, "internal_port = 8080")?;
    require_fragment(raw, "force_https = true")?;
    require_fragment(raw, "auto_start_machines = true")?;
    require_fragment(raw, "min_machines_running = 1")?;
    require_fragment(raw, "path = \"/gateway/status\"")?;
    Ok(())
}

/// Public `fn` `load_deployment_ops_runbook` in `tau-deployment`.
pub fn load_deployment_ops_runbook() -> Result<String> {
    let path = repo_root().join(DEPLOYMENT_RUNBOOK_RELATIVE_PATH);
    std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read deployment runbook {}", path.display()))
}

/// Public `fn` `validate_fly_runbook_contract` in `tau-deployment`.
pub fn validate_fly_runbook_contract(raw: &str) -> Result<()> {
    require_fragment(raw, "fly.toml")?;
    require_fragment(raw, "fly launch")?;
    require_fragment(raw, "fly deploy")?;
    require_fragment(raw, "fly status")?;
    require_fragment(raw, "fly logs")?;
    Ok(())
}

fn require_fragment(raw: &str, fragment: &str) -> Result<()> {
    if raw.contains(fragment) {
        return Ok(());
    }
    bail!("missing required fragment: {fragment}");
}

#[cfg(test)]
mod tests {
    use super::{
        load_deployment_ops_runbook, load_repo_fly_manifest, validate_fly_manifest_contract,
        validate_fly_runbook_contract,
    };

    #[test]
    fn spec_c01_load_repo_fly_manifest_contains_gateway_startup_contract() {
        let manifest = load_repo_fly_manifest().expect("fly manifest should load");
        validate_fly_manifest_contract(&manifest).expect("fly manifest contract should validate");
    }

    #[test]
    fn spec_c02_validate_fly_manifest_contract_requires_service_and_health_checks() {
        let invalid_manifest = r#"
[build]
dockerfile = "Dockerfile"

[env]
TAU_TRANSPORT_MODE = "gateway"
TAU_GATEWAY_OPENRESPONSES_SERVER = "true"
TAU_GATEWAY_OPENRESPONSES_BIND = "0.0.0.0:8080"

[http_service]
internal_port = 8080
force_https = true
auto_start_machines = true
min_machines_running = 1
"#;
        let error = validate_fly_manifest_contract(invalid_manifest)
            .expect_err("manifest without checks should fail");
        assert!(
            error
                .to_string()
                .contains("missing required fragment: [[http_service.checks]]"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn regression_spec_c03_deployment_runbook_documents_fly_workflow() {
        let runbook = load_deployment_ops_runbook().expect("deployment runbook should load");
        validate_fly_runbook_contract(&runbook).expect("runbook should include fly workflow");
    }
}
