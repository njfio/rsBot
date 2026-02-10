use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use wasmparser::{Parser, Payload};

use crate::{current_unix_timestamp_ms, write_text_atomic, Cli};

pub(crate) const DEPLOYMENT_WASM_MANIFEST_SCHEMA_VERSION: u32 = 1;
pub(crate) const DEPLOYMENT_WASM_MANIFEST_KIND: &str = "tau_wasm_deliverable";
pub(crate) const DEPLOYMENT_WASM_PROFILE_SCHEMA_VERSION: u32 = 1;
pub(crate) const DEPLOYMENT_WASM_CONTROL_PLANE_PROFILE_ID: &str = "control_plane_gateway_v1";
const DEPLOYMENT_WASM_MODULE_MAGIC: [u8; 4] = [0x00, 0x61, 0x73, 0x6d];
const DEPLOYMENT_WASM_DEFAULT_MAX_ARTIFACT_SIZE_BYTES: u64 = 2 * 1024 * 1024;

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

fn default_required_feature_gates() -> Vec<String> {
    vec![
        "no_native_process_exec".to_string(),
        "filesystem_limited_to_preopened_dirs".to_string(),
        "network_access_requires_host_capability".to_string(),
        "deterministic_time_requires_host_injection".to_string(),
    ]
}

fn default_allowed_import_modules() -> Vec<String> {
    vec!["wasi_snapshot_preview1".to_string(), "env".to_string()]
}

fn default_forbidden_import_modules() -> Vec<String> {
    vec!["wasi_unstable".to_string()]
}

fn default_runtime_constraints() -> DeploymentWasmRuntimeConstraintProfile {
    DeploymentWasmRuntimeConstraintProfile {
        schema_version: DEPLOYMENT_WASM_PROFILE_SCHEMA_VERSION,
        profile_id: DEPLOYMENT_WASM_CONTROL_PLANE_PROFILE_ID.to_string(),
        target_role: "control_plane_gateway".to_string(),
        required_runtime_profile: "wasm_wasi".to_string(),
        required_abi: "wasi_snapshot_preview1".to_string(),
        required_feature_gates: default_required_feature_gates(),
        allowed_import_modules: default_allowed_import_modules(),
        forbidden_import_modules: default_forbidden_import_modules(),
        max_artifact_size_bytes: DEPLOYMENT_WASM_DEFAULT_MAX_ARTIFACT_SIZE_BYTES,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct DeploymentWasmRuntimeConstraintProfile {
    pub(crate) schema_version: u32,
    pub(crate) profile_id: String,
    pub(crate) target_role: String,
    pub(crate) required_runtime_profile: String,
    pub(crate) required_abi: String,
    #[serde(default = "default_required_feature_gates")]
    pub(crate) required_feature_gates: Vec<String>,
    #[serde(default = "default_allowed_import_modules")]
    pub(crate) allowed_import_modules: Vec<String>,
    #[serde(default = "default_forbidden_import_modules")]
    pub(crate) forbidden_import_modules: Vec<String>,
    pub(crate) max_artifact_size_bytes: u64,
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
    #[serde(default = "default_runtime_constraints")]
    pub(crate) runtime_constraints: DeploymentWasmRuntimeConstraintProfile,
    #[serde(default)]
    pub(crate) observed_import_modules: Vec<String>,
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
    pub(crate) constraint_profile_id: String,
    pub(crate) compliance_reason_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct DeploymentWasmInspectReport {
    pub(crate) manifest_path: String,
    pub(crate) blueprint_id: String,
    pub(crate) runtime_profile: String,
    pub(crate) constraint_profile_id: String,
    pub(crate) compliant: bool,
    pub(crate) reason_codes: Vec<String>,
    pub(crate) observed_import_modules: Vec<String>,
    pub(crate) required_feature_gates: Vec<String>,
    pub(crate) max_artifact_size_bytes: u64,
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
    validate_runtime_constraint_profile(&manifest.runtime_constraints, &manifest.runtime_profile)?;
    let compliance = evaluate_runtime_constraint_compliance(
        &manifest.runtime_constraints,
        &manifest.runtime_profile,
        manifest.artifact_size_bytes,
        &manifest.observed_import_modules,
        &manifest.capability_constraints,
    );
    if !compliance.compliant {
        bail!(
            "deployment wasm manifest runtime constraints are not compliant: reason_codes={}",
            compliance.reason_codes.join(",")
        );
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
    let observed_import_modules =
        collect_wasm_import_modules(&artifact_bytes).with_context(|| {
            format!(
                "failed to inspect deployment wasm artifact imports {}",
                artifact_path.display()
            )
        })?;
    let compliance = evaluate_runtime_constraint_compliance(
        &manifest.runtime_constraints,
        &manifest.runtime_profile,
        manifest.artifact_size_bytes,
        &observed_import_modules,
        &manifest.capability_constraints,
    );
    if !compliance.compliant {
        bail!(
            "deployment wasm manifest runtime constraints failed: reason_codes={}",
            compliance.reason_codes.join(",")
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
    let observed_import_modules = collect_wasm_import_modules(&module_bytes)
        .with_context(|| format!("failed to inspect wasm imports {}", module_path.display()))?;
    let artifact_sha256 = sha256_hex(&module_bytes);
    let artifact_size_bytes = u64::try_from(module_bytes.len()).unwrap_or(u64::MAX);
    let runtime_constraints =
        resolve_runtime_constraint_profile_for_runtime(config.runtime_profile.trim());
    validate_runtime_constraint_profile(&runtime_constraints, config.runtime_profile.trim())?;
    let capability_constraints = default_capability_constraints();
    let compliance = evaluate_runtime_constraint_compliance(
        &runtime_constraints,
        config.runtime_profile.trim(),
        artifact_size_bytes,
        &observed_import_modules,
        &capability_constraints,
    );
    if !compliance.compliant {
        bail!(
            "deployment wasm package runtime constraints failed: reason_codes={}",
            compliance.reason_codes.join(",")
        );
    }

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
        capability_constraints: capability_constraints.clone(),
        runtime_constraints: runtime_constraints.clone(),
        observed_import_modules: observed_import_modules.clone(),
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
        constraint_profile_id: runtime_constraints.profile_id,
        compliance_reason_codes: compliance.reason_codes,
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
        "deployment wasm package: blueprint_id={} runtime_profile={} artifact_path={} artifact_sha256={} artifact_size_bytes={} manifest_path={} state_path={} state_updated={} constraint_profile_id={} compliance_reason_codes={} constraints={}",
        report.blueprint_id,
        report.runtime_profile,
        report.artifact_path,
        report.artifact_sha256,
        report.artifact_size_bytes,
        report.manifest_path,
        report.state_path,
        report.state_updated,
        report.constraint_profile_id,
        if report.compliance_reason_codes.is_empty() {
            "none".to_string()
        } else {
            report.compliance_reason_codes.join(",")
        },
        report.capability_constraints.join(",")
    )
}

pub(crate) fn execute_deployment_wasm_inspect_command(cli: &Cli) -> Result<()> {
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

pub(crate) fn inspect_deployment_wasm_deliverable(
    path: &Path,
) -> Result<DeploymentWasmInspectReport> {
    let manifest = load_deployment_wasm_manifest(path)?;
    let artifact_path = resolve_manifest_artifact_path(path, &manifest.artifact_path);
    let bytes = std::fs::read(&artifact_path).with_context(|| {
        format!(
            "failed to read deployment wasm artifact {}",
            artifact_path.display()
        )
    })?;
    let observed_import_modules = collect_wasm_import_modules(&bytes)?;
    let compliance = evaluate_runtime_constraint_compliance(
        &manifest.runtime_constraints,
        &manifest.runtime_profile,
        manifest.artifact_size_bytes,
        &observed_import_modules,
        &manifest.capability_constraints,
    );
    Ok(DeploymentWasmInspectReport {
        manifest_path: path.display().to_string(),
        blueprint_id: manifest.blueprint_id,
        runtime_profile: manifest.runtime_profile,
        constraint_profile_id: manifest.runtime_constraints.profile_id,
        compliant: compliance.compliant,
        reason_codes: compliance.reason_codes,
        observed_import_modules,
        required_feature_gates: manifest.runtime_constraints.required_feature_gates,
        max_artifact_size_bytes: manifest.runtime_constraints.max_artifact_size_bytes,
    })
}

pub(crate) fn render_deployment_wasm_inspect_report(
    report: &DeploymentWasmInspectReport,
) -> String {
    format!(
        "deployment wasm inspect: manifest_path={} blueprint_id={} runtime_profile={} constraint_profile_id={} compliant={} reason_codes={} observed_import_modules={} required_feature_gates={} max_artifact_size_bytes={}",
        report.manifest_path,
        report.blueprint_id,
        report.runtime_profile,
        report.constraint_profile_id,
        report.compliant,
        if report.reason_codes.is_empty() {
            "none".to_string()
        } else {
            report.reason_codes.join(",")
        },
        if report.observed_import_modules.is_empty() {
            "none".to_string()
        } else {
            report.observed_import_modules.join(",")
        },
        if report.required_feature_gates.is_empty() {
            "none".to_string()
        } else {
            report.required_feature_gates.join(",")
        },
        report.max_artifact_size_bytes
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeConstraintCompliance {
    compliant: bool,
    reason_codes: Vec<String>,
}

fn resolve_runtime_constraint_profile_for_runtime(
    runtime_profile: &str,
) -> DeploymentWasmRuntimeConstraintProfile {
    let mut profile = default_runtime_constraints();
    profile.required_runtime_profile = runtime_profile.trim().to_string();
    profile
}

fn validate_runtime_constraint_profile(
    profile: &DeploymentWasmRuntimeConstraintProfile,
    runtime_profile: &str,
) -> Result<()> {
    if profile.schema_version != DEPLOYMENT_WASM_PROFILE_SCHEMA_VERSION {
        bail!(
            "unsupported deployment wasm profile schema {} (expected {})",
            profile.schema_version,
            DEPLOYMENT_WASM_PROFILE_SCHEMA_VERSION
        );
    }
    if profile.profile_id.trim().is_empty() {
        bail!("deployment wasm runtime constraint profile_id cannot be empty");
    }
    if profile.target_role.trim() != "control_plane_gateway" {
        bail!(
            "deployment wasm runtime constraint target_role must be 'control_plane_gateway' (found '{}')",
            profile.target_role
        );
    }
    if profile.required_runtime_profile.trim() != runtime_profile.trim() {
        bail!(
            "deployment wasm runtime constraint required_runtime_profile '{}' does not match runtime profile '{}'",
            profile.required_runtime_profile,
            runtime_profile
        );
    }
    if profile.required_abi.trim().is_empty() {
        bail!("deployment wasm runtime constraint required_abi cannot be empty");
    }
    if profile.required_feature_gates.is_empty() {
        bail!("deployment wasm runtime constraint required_feature_gates cannot be empty");
    }
    if profile
        .required_feature_gates
        .iter()
        .any(|gate| gate.trim().is_empty())
    {
        bail!(
            "deployment wasm runtime constraint required_feature_gates cannot contain empty values"
        );
    }
    if profile.max_artifact_size_bytes == 0 {
        bail!("deployment wasm runtime constraint max_artifact_size_bytes must be greater than 0");
    }
    Ok(())
}

fn evaluate_runtime_constraint_compliance(
    profile: &DeploymentWasmRuntimeConstraintProfile,
    runtime_profile: &str,
    artifact_size_bytes: u64,
    import_modules: &[String],
    capability_constraints: &[String],
) -> RuntimeConstraintCompliance {
    let mut reason_codes = Vec::new();

    if profile.required_runtime_profile.trim() != runtime_profile.trim() {
        reason_codes.push("runtime_profile_mismatch".to_string());
    }
    if artifact_size_bytes > profile.max_artifact_size_bytes {
        reason_codes.push("artifact_size_limit_exceeded".to_string());
    }
    if profile
        .required_feature_gates
        .iter()
        .any(|required| !capability_constraints.iter().any(|value| value == required))
    {
        reason_codes.push("required_feature_gate_missing".to_string());
    }
    if import_modules.is_empty() {
        reason_codes.push("abi_assumed_no_imports".to_string());
    } else if !import_modules
        .iter()
        .any(|module| module == &profile.required_abi)
    {
        reason_codes.push("required_abi_missing".to_string());
    }
    if !profile.allowed_import_modules.is_empty()
        && import_modules
            .iter()
            .any(|module| !profile.allowed_import_modules.contains(module))
    {
        reason_codes.push("import_module_not_allowlisted".to_string());
    }
    if import_modules
        .iter()
        .any(|module| profile.forbidden_import_modules.contains(module))
    {
        reason_codes.push("import_module_forbidden".to_string());
    }

    let compliant = reason_codes
        .iter()
        .all(|code| code == "abi_assumed_no_imports");
    RuntimeConstraintCompliance {
        compliant,
        reason_codes,
    }
}

fn collect_wasm_import_modules(bytes: &[u8]) -> Result<Vec<String>> {
    let mut modules = BTreeSet::new();
    for payload in Parser::new(0).parse_all(bytes) {
        let payload = payload.context("failed to parse wasm payload while collecting imports")?;
        if let Payload::ImportSection(section) = payload {
            for entry in section {
                let entry = entry.context("failed to parse wasm import section entry")?;
                modules.insert(entry.module.to_string());
            }
        }
    }
    Ok(modules.into_iter().collect())
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
        "constraint_profile_id": manifest.runtime_constraints.profile_id,
        "required_abi": manifest.runtime_constraints.required_abi,
        "required_feature_gates": manifest.runtime_constraints.required_feature_gates,
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
        inspect_deployment_wasm_deliverable, load_deployment_wasm_manifest,
        package_deployment_wasm_artifact, parse_deployment_wasm_manifest,
        render_deployment_wasm_inspect_report, validate_deployment_wasm_manifest,
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
            runtime_constraints: super::default_runtime_constraints(),
            observed_import_modules: vec![],
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
        assert_eq!(report.constraint_profile_id, "control_plane_gateway_v1");
        assert!(report
            .compliance_reason_codes
            .iter()
            .all(|code| code == "abi_assumed_no_imports"));
    }

    #[test]
    fn unit_validate_deployment_wasm_manifest_rejects_invalid_runtime_constraint_profile() {
        let mut manifest = DeploymentWasmArtifactManifest {
            schema_version: 1,
            manifest_kind: "tau_wasm_deliverable".to_string(),
            blueprint_id: "edge-wasm".to_string(),
            deploy_target: "wasm".to_string(),
            runtime_profile: "wasm_wasi".to_string(),
            source_module_path: "/tmp/module.wasm".to_string(),
            artifact_path: "artifact.wasm".to_string(),
            artifact_sha256: "1b4884ca4f2513378fa87f2f3d784f4f3f6f7f1ef3f0af6d0ce31fc89f8f8f02"
                .to_string(),
            artifact_size_bytes: 8,
            generated_unix_ms: 1,
            capability_constraints: vec![
                "no_native_process_exec".to_string(),
                "filesystem_limited_to_preopened_dirs".to_string(),
                "network_access_requires_host_capability".to_string(),
                "deterministic_time_requires_host_injection".to_string(),
            ],
            runtime_constraints: super::default_runtime_constraints(),
            observed_import_modules: vec![],
        };
        manifest.runtime_constraints.max_artifact_size_bytes = 0;
        let error =
            validate_deployment_wasm_manifest(&manifest).expect_err("invalid profile should fail");
        assert!(error
            .to_string()
            .contains("max_artifact_size_bytes must be greater than 0"));
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
        assert_eq!(
            manifest.runtime_constraints.profile_id,
            "control_plane_gateway_v1"
        );
    }

    #[test]
    fn integration_inspect_deployment_wasm_deliverable_reports_compliance_status() {
        let temp = tempdir().expect("tempdir");
        let module_path = temp.path().join("edge.wasm");
        write_test_wasm_module(&module_path);
        let report = package_deployment_wasm_artifact(&DeploymentWasmPackageConfig {
            module_path,
            blueprint_id: "edge-wasm-inspect".to_string(),
            runtime_profile: "wasm_wasi".to_string(),
            output_dir: temp.path().join("out"),
            state_dir: temp.path().join(".tau/deployment"),
        })
        .expect("package wasm");

        let inspect = inspect_deployment_wasm_deliverable(Path::new(&report.manifest_path))
            .expect("inspect manifest");
        assert!(inspect.compliant);
        assert_eq!(inspect.constraint_profile_id, "control_plane_gateway_v1");
        assert!(inspect
            .reason_codes
            .iter()
            .all(|code| code == "abi_assumed_no_imports"));
        let rendered = render_deployment_wasm_inspect_report(&inspect);
        assert!(rendered.contains("deployment wasm inspect:"));
        assert!(rendered.contains("constraint_profile_id=control_plane_gateway_v1"));
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

    #[test]
    fn regression_inspect_deployment_wasm_deliverable_fails_on_constraint_drift() {
        let temp = tempdir().expect("tempdir");
        let module_path = temp.path().join("edge.wasm");
        write_test_wasm_module(&module_path);
        let report = package_deployment_wasm_artifact(&DeploymentWasmPackageConfig {
            module_path,
            blueprint_id: "edge-wasm-drift".to_string(),
            runtime_profile: "wasm_wasi".to_string(),
            output_dir: temp.path().join("out"),
            state_dir: temp.path().join(".tau/deployment"),
        })
        .expect("package wasm");

        let mut manifest_json: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&report.manifest_path).expect("read manifest"),
        )
        .expect("parse manifest json");
        manifest_json["runtime_constraints"]["max_artifact_size_bytes"] = serde_json::json!(1);
        std::fs::write(
            &report.manifest_path,
            serde_json::to_string_pretty(&manifest_json).expect("encode drifted manifest"),
        )
        .expect("write drifted manifest");

        let error = inspect_deployment_wasm_deliverable(Path::new(&report.manifest_path))
            .expect_err("constraint drift should fail");
        assert!(error
            .to_string()
            .contains("invalid deployment wasm manifest"));
    }
}
