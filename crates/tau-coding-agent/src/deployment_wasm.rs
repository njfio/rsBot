use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{current_unix_timestamp_ms, write_text_atomic, Cli};

pub(crate) const DEPLOYMENT_WASM_MANIFEST_SCHEMA_VERSION: u32 = 1;
pub(crate) const DEPLOYMENT_WASM_MANIFEST_KIND: &str = "tau_wasm_deliverable";
const DEPLOYMENT_WASM_MODULE_MAGIC: [u8; 4] = [0x00, 0x61, 0x73, 0x6d];

fn deployment_wasm_manifest_schema_version() -> u32 {
    DEPLOYMENT_WASM_MANIFEST_SCHEMA_VERSION
}

fn default_manifest_kind() -> String {
    DEPLOYMENT_WASM_MANIFEST_KIND.to_string()
}

fn default_capability_constraints() -> Vec<String> {
    vec![
        "no_native_process_exec".to_string(),
        "filesystem_limited_to_preopened_dirs".to_string(),
        "network_access_requires_host_capability".to_string(),
        "deterministic_time_requires_host_injection".to_string(),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct DeploymentWasmArtifactManifest {
    #[serde(default = "deployment_wasm_manifest_schema_version")]
    pub(crate) schema_version: u32,
    #[serde(default = "default_manifest_kind")]
    pub(crate) manifest_kind: String,
    pub(crate) blueprint_id: String,
    pub(crate) deploy_target: String,
    pub(crate) runtime_profile: String,
    pub(crate) source_module_path: String,
    pub(crate) artifact_path: String,
    pub(crate) artifact_sha256: String,
    pub(crate) artifact_size_bytes: u64,
    pub(crate) generated_unix_ms: u64,
    #[serde(default = "default_capability_constraints")]
    pub(crate) capability_constraints: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct DeploymentWasmPackageConfig {
    pub(crate) module_path: PathBuf,
    pub(crate) blueprint_id: String,
    pub(crate) runtime_profile: String,
    pub(crate) output_dir: PathBuf,
    pub(crate) state_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct DeploymentWasmPackageReport {
    pub(crate) blueprint_id: String,
    pub(crate) runtime_profile: String,
    pub(crate) source_module_path: String,
    pub(crate) artifact_path: String,
    pub(crate) artifact_sha256: String,
    pub(crate) artifact_size_bytes: u64,
    pub(crate) manifest_path: String,
    pub(crate) state_path: String,
    pub(crate) state_updated: bool,
    pub(crate) capability_constraints: Vec<String>,
}

pub(crate) fn parse_deployment_wasm_manifest(raw: &str) -> Result<DeploymentWasmArtifactManifest> {
    let manifest = serde_json::from_str::<DeploymentWasmArtifactManifest>(raw)
        .context("failed to parse deployment wasm manifest")?;
    validate_deployment_wasm_manifest(&manifest)?;
    Ok(manifest)
}

pub(crate) fn validate_deployment_wasm_manifest(
    manifest: &DeploymentWasmArtifactManifest,
) -> Result<()> {
    if manifest.schema_version != DEPLOYMENT_WASM_MANIFEST_SCHEMA_VERSION {
        bail!(
            "unsupported deployment wasm manifest schema {} (expected {})",
            manifest.schema_version,
            DEPLOYMENT_WASM_MANIFEST_SCHEMA_VERSION
        );
    }
    if manifest.manifest_kind.trim() != DEPLOYMENT_WASM_MANIFEST_KIND {
        bail!(
            "unsupported deployment wasm manifest kind '{}' (expected '{}')",
            manifest.manifest_kind,
            DEPLOYMENT_WASM_MANIFEST_KIND
        );
    }
    if manifest.blueprint_id.trim().is_empty() {
        bail!("deployment wasm manifest blueprint_id cannot be empty");
    }
    if manifest.deploy_target.trim() != "wasm" {
        bail!(
            "deployment wasm manifest deploy_target must be 'wasm' (found '{}')",
            manifest.deploy_target
        );
    }
    if !is_supported_wasm_runtime_profile(manifest.runtime_profile.trim()) {
        bail!(
            "unsupported deployment wasm runtime profile '{}'",
            manifest.runtime_profile
        );
    }
    if manifest.source_module_path.trim().is_empty() {
        bail!("deployment wasm manifest source_module_path cannot be empty");
    }
    if manifest.artifact_path.trim().is_empty() {
        bail!("deployment wasm manifest artifact_path cannot be empty");
    }
    if !is_valid_sha256_hex(manifest.artifact_sha256.trim()) {
        bail!("deployment wasm manifest artifact_sha256 must be a 64-char lowercase hex string");
    }
    if manifest.artifact_size_bytes == 0 {
        bail!("deployment wasm manifest artifact_size_bytes must be greater than 0");
    }
    if manifest.generated_unix_ms == 0 {
        bail!("deployment wasm manifest generated_unix_ms must be greater than 0");
    }
    if manifest.capability_constraints.is_empty() {
        bail!("deployment wasm manifest capability_constraints cannot be empty");
    }
    if manifest
        .capability_constraints
        .iter()
        .any(|constraint| constraint.trim().is_empty())
    {
        bail!("deployment wasm manifest capability_constraints cannot contain empty values");
    }
    Ok(())
}

pub(crate) fn load_deployment_wasm_manifest(path: &Path) -> Result<DeploymentWasmArtifactManifest> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read deployment wasm manifest {}", path.display()))?;
    let manifest = parse_deployment_wasm_manifest(&raw)
        .with_context(|| format!("invalid deployment wasm manifest {}", path.display()))?;
    let artifact_path = resolve_manifest_artifact_path(path, &manifest.artifact_path);
    let artifact_bytes = std::fs::read(&artifact_path).with_context(|| {
        format!(
            "failed to read deployment wasm artifact {}",
            artifact_path.display()
        )
    })?;
    validate_wasm_module_bytes(&artifact_bytes).with_context(|| {
        format!(
            "invalid deployment wasm artifact {}",
            artifact_path.display()
        )
    })?;
    let observed_hash = sha256_hex(&artifact_bytes);
    if observed_hash != manifest.artifact_sha256 {
        bail!(
            "deployment wasm manifest hash mismatch: expected {} observed {}",
            manifest.artifact_sha256,
            observed_hash
        );
    }
    let observed_size = u64::try_from(artifact_bytes.len()).unwrap_or(u64::MAX);
    if observed_size != manifest.artifact_size_bytes {
        bail!(
            "deployment wasm manifest size mismatch: expected {} observed {}",
            manifest.artifact_size_bytes,
            observed_size
        );
    }
    Ok(manifest)
}

pub(crate) fn package_deployment_wasm_artifact(
    config: &DeploymentWasmPackageConfig,
) -> Result<DeploymentWasmPackageReport> {
    let module_path = &config.module_path;
    if !module_path.exists() {
        bail!(
            "--deployment-wasm-package-module '{}' does not exist",
            module_path.display()
        );
    }
    if !module_path.is_file() {
        bail!(
            "--deployment-wasm-package-module '{}' must point to a file",
            module_path.display()
        );
    }
    if config.blueprint_id.trim().is_empty() {
        bail!("--deployment-wasm-package-blueprint-id cannot be empty");
    }
    if !is_supported_wasm_runtime_profile(config.runtime_profile.trim()) {
        bail!(
            "unsupported deployment wasm runtime profile '{}'",
            config.runtime_profile
        );
    }

    let module_bytes = std::fs::read(module_path)
        .with_context(|| format!("failed to read {}", module_path.display()))?;
    validate_wasm_module_bytes(&module_bytes)
        .with_context(|| format!("invalid wasm module {}", module_path.display()))?;
    let artifact_sha256 = sha256_hex(&module_bytes);
    let artifact_size_bytes = u64::try_from(module_bytes.len()).unwrap_or(u64::MAX);

    let artifact_root = config
        .output_dir
        .join(sanitize_for_path(config.blueprint_id.trim()));
    std::fs::create_dir_all(&artifact_root)
        .with_context(|| format!("failed to create {}", artifact_root.display()))?;

    let artifact_file_name = format!("{}.wasm", artifact_sha256);
    let artifact_path = artifact_root.join(&artifact_file_name);
    std::fs::write(&artifact_path, &module_bytes)
        .with_context(|| format!("failed to write {}", artifact_path.display()))?;

    let manifest_file_name = format!("{}.manifest.json", artifact_sha256);
    let manifest_path = artifact_root.join(&manifest_file_name);
    let manifest = DeploymentWasmArtifactManifest {
        schema_version: DEPLOYMENT_WASM_MANIFEST_SCHEMA_VERSION,
        manifest_kind: DEPLOYMENT_WASM_MANIFEST_KIND.to_string(),
        blueprint_id: config.blueprint_id.trim().to_string(),
        deploy_target: "wasm".to_string(),
        runtime_profile: config.runtime_profile.trim().to_string(),
        source_module_path: module_path.display().to_string(),
        artifact_path: artifact_file_name,
        artifact_sha256: artifact_sha256.clone(),
        artifact_size_bytes,
        generated_unix_ms: current_unix_timestamp_ms(),
        capability_constraints: default_capability_constraints(),
    };
    let manifest_payload = serde_json::to_string_pretty(&manifest)
        .context("failed to serialize deployment wasm manifest")?;
    write_text_atomic(&manifest_path, &manifest_payload)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;

    let state_path = config.state_dir.join("state.json");
    let state_updated =
        upsert_state_with_wasm_deliverable(&state_path, &manifest, &manifest_path, &artifact_path)?;

    Ok(DeploymentWasmPackageReport {
        blueprint_id: manifest.blueprint_id,
        runtime_profile: manifest.runtime_profile,
        source_module_path: module_path.display().to_string(),
        artifact_path: artifact_path.display().to_string(),
        artifact_sha256,
        artifact_size_bytes,
        manifest_path: manifest_path.display().to_string(),
        state_path: state_path.display().to_string(),
        state_updated,
        capability_constraints: manifest.capability_constraints,
    })
}

pub(crate) fn execute_deployment_wasm_package_command(cli: &Cli) -> Result<()> {
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

pub(crate) fn render_deployment_wasm_package_report(
    report: &DeploymentWasmPackageReport,
) -> String {
    format!(
        "deployment wasm package: blueprint_id={} runtime_profile={} artifact_path={} artifact_sha256={} artifact_size_bytes={} manifest_path={} state_path={} state_updated={} constraints={}",
        report.blueprint_id,
        report.runtime_profile,
        report.artifact_path,
        report.artifact_sha256,
        report.artifact_size_bytes,
        report.manifest_path,
        report.state_path,
        report.state_updated,
        report.capability_constraints.join(",")
    )
}

fn is_supported_wasm_runtime_profile(profile: &str) -> bool {
    matches!(profile.trim(), "wasm_wasi")
}

fn validate_wasm_module_bytes(bytes: &[u8]) -> Result<()> {
    if bytes.len() < DEPLOYMENT_WASM_MODULE_MAGIC.len() {
        bail!("invalid wasm module: file is too small");
    }
    if bytes[..DEPLOYMENT_WASM_MODULE_MAGIC.len()] != DEPLOYMENT_WASM_MODULE_MAGIC {
        bail!("invalid wasm module magic");
    }
    Ok(())
}

fn is_valid_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn resolve_manifest_artifact_path(manifest_path: &Path, artifact_path: &str) -> PathBuf {
    let artifact_path = PathBuf::from(artifact_path);
    if artifact_path.is_absolute() {
        return artifact_path;
    }
    manifest_path
        .parent()
        .map(|parent| parent.join(&artifact_path))
        .unwrap_or(artifact_path)
}

fn sanitize_for_path(raw: &str) -> String {
    raw.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn default_deployment_state_payload() -> serde_json::Value {
    json!({
        "schema_version": 1,
        "processed_case_keys": [],
        "rollouts": [],
        "health": {},
        "wasm_deliverables": [],
    })
}

fn upsert_state_with_wasm_deliverable(
    state_path: &Path,
    manifest: &DeploymentWasmArtifactManifest,
    manifest_path: &Path,
    artifact_path: &Path,
) -> Result<bool> {
    if let Some(parent) = state_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }

    let mut state = if state_path.exists() {
        let raw = std::fs::read_to_string(state_path)
            .with_context(|| format!("failed to read {}", state_path.display()))?;
        serde_json::from_str::<serde_json::Value>(&raw)
            .with_context(|| format!("failed to parse {}", state_path.display()))?
    } else {
        default_deployment_state_payload()
    };

    let Some(state_obj) = state.as_object_mut() else {
        bail!(
            "deployment state {} must be a JSON object",
            state_path.display()
        );
    };
    if !state_obj.contains_key("schema_version") {
        state_obj.insert("schema_version".to_string(), json!(1));
    }
    state_obj
        .entry("processed_case_keys".to_string())
        .or_insert_with(|| json!([]));
    state_obj
        .entry("rollouts".to_string())
        .or_insert_with(|| json!([]));
    state_obj
        .entry("health".to_string())
        .or_insert_with(|| json!({}));

    let deliverables = state_obj
        .entry("wasm_deliverables".to_string())
        .or_insert_with(|| json!([]));
    let Some(deliverables_array) = deliverables.as_array_mut() else {
        bail!(
            "deployment state {} field 'wasm_deliverables' must be an array",
            state_path.display()
        );
    };

    let entry = json!({
        "schema_version": DEPLOYMENT_WASM_MANIFEST_SCHEMA_VERSION,
        "blueprint_id": manifest.blueprint_id,
        "deploy_target": manifest.deploy_target,
        "runtime_profile": manifest.runtime_profile,
        "artifact_path": artifact_path.display().to_string(),
        "artifact_sha256": manifest.artifact_sha256,
        "artifact_size_bytes": manifest.artifact_size_bytes,
        "manifest_path": manifest_path.display().to_string(),
        "generated_unix_ms": manifest.generated_unix_ms,
    });

    let mut replaced = false;
    for current in deliverables_array.iter_mut() {
        let same_blueprint = current
            .get("blueprint_id")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| value == manifest.blueprint_id);
        let same_runtime = current
            .get("runtime_profile")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| value == manifest.runtime_profile);
        if same_blueprint && same_runtime {
            *current = entry.clone();
            replaced = true;
            break;
        }
    }
    if !replaced {
        deliverables_array.push(entry);
    }

    let payload =
        serde_json::to_string_pretty(&state).context("failed to serialize deployment state")?;
    write_text_atomic(state_path, &payload)
        .with_context(|| format!("failed to write {}", state_path.display()))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::{
        load_deployment_wasm_manifest, package_deployment_wasm_artifact,
        parse_deployment_wasm_manifest, validate_deployment_wasm_manifest,
        DeploymentWasmArtifactManifest, DeploymentWasmPackageConfig,
    };
    use std::path::Path;
    use tempfile::tempdir;

    fn write_test_wasm_module(path: &Path) {
        let bytes = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        std::fs::write(path, bytes).expect("write wasm module");
    }

    #[test]
    fn unit_validate_deployment_wasm_manifest_rejects_invalid_target() {
        let manifest = DeploymentWasmArtifactManifest {
            schema_version: 1,
            manifest_kind: "tau_wasm_deliverable".to_string(),
            blueprint_id: "edge-wasm".to_string(),
            deploy_target: "container".to_string(),
            runtime_profile: "wasm_wasi".to_string(),
            source_module_path: "/tmp/module.wasm".to_string(),
            artifact_path: "artifact.wasm".to_string(),
            artifact_sha256: "1b4884ca4f2513378fa87f2f3d784f4f3f6f7f1ef3f0af6d0ce31fc89f8f8f02"
                .to_string(),
            artifact_size_bytes: 8,
            generated_unix_ms: 1,
            capability_constraints: vec!["no_native_process_exec".to_string()],
        };
        let error = validate_deployment_wasm_manifest(&manifest).expect_err("invalid target");
        assert!(error.to_string().contains("deploy_target must be 'wasm'"));
    }

    #[test]
    fn functional_package_deployment_wasm_artifact_writes_manifest_and_state() {
        let temp = tempdir().expect("tempdir");
        let module_path = temp.path().join("edge.wasm");
        write_test_wasm_module(&module_path);

        let report = package_deployment_wasm_artifact(&DeploymentWasmPackageConfig {
            module_path: module_path.clone(),
            blueprint_id: "edge-wasm".to_string(),
            runtime_profile: "wasm_wasi".to_string(),
            output_dir: temp.path().join("out"),
            state_dir: temp.path().join(".tau/deployment"),
        })
        .expect("package wasm");
        assert!(Path::new(&report.artifact_path).exists());
        assert!(Path::new(&report.manifest_path).exists());
        assert!(Path::new(&report.state_path).exists());
        assert_eq!(report.artifact_size_bytes, 8);
    }

    #[test]
    fn integration_load_deployment_wasm_manifest_verifies_hash_and_constraints() {
        let temp = tempdir().expect("tempdir");
        let module_path = temp.path().join("edge.wasm");
        write_test_wasm_module(&module_path);

        let report = package_deployment_wasm_artifact(&DeploymentWasmPackageConfig {
            module_path,
            blueprint_id: "edge-wasm".to_string(),
            runtime_profile: "wasm_wasi".to_string(),
            output_dir: temp.path().join("out"),
            state_dir: temp.path().join(".tau/deployment"),
        })
        .expect("package wasm");

        let manifest =
            load_deployment_wasm_manifest(Path::new(&report.manifest_path)).expect("load manifest");
        assert_eq!(manifest.runtime_profile, "wasm_wasi");
        assert_eq!(manifest.artifact_sha256, report.artifact_sha256);
        assert!(!manifest.capability_constraints.is_empty());
    }

    #[test]
    fn regression_package_deployment_wasm_artifact_rejects_non_wasm_binary() {
        let temp = tempdir().expect("tempdir");
        let module_path = temp.path().join("not-wasm.bin");
        std::fs::write(&module_path, b"not a wasm module").expect("write invalid payload");

        let error = package_deployment_wasm_artifact(&DeploymentWasmPackageConfig {
            module_path,
            blueprint_id: "edge-wasm".to_string(),
            runtime_profile: "wasm_wasi".to_string(),
            output_dir: temp.path().join("out"),
            state_dir: temp.path().join(".tau/deployment"),
        })
        .expect_err("invalid binary should fail");
        assert!(error.to_string().contains("invalid wasm module"));
    }

    #[test]
    fn regression_load_deployment_wasm_manifest_rejects_hash_mismatch() {
        let temp = tempdir().expect("tempdir");
        let module_path = temp.path().join("edge.wasm");
        write_test_wasm_module(&module_path);
        let report = package_deployment_wasm_artifact(&DeploymentWasmPackageConfig {
            module_path,
            blueprint_id: "edge-wasm".to_string(),
            runtime_profile: "wasm_wasi".to_string(),
            output_dir: temp.path().join("out"),
            state_dir: temp.path().join(".tau/deployment"),
        })
        .expect("package wasm");
        std::fs::write(&report.artifact_path, b"\x00asm\x01\x00\x00\x01").expect("tamper artifact");

        let error = load_deployment_wasm_manifest(Path::new(&report.manifest_path))
            .expect_err("hash mismatch should fail");
        assert!(error.to_string().contains("hash mismatch"));
    }

    #[test]
    fn regression_parse_deployment_wasm_manifest_rejects_invalid_json() {
        let error = parse_deployment_wasm_manifest("{not-json").expect_err("invalid json");
        assert!(error
            .to_string()
            .contains("failed to parse deployment wasm manifest"));
    }
}
