use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tau_cli::Cli;
use tau_core::{current_unix_timestamp, is_expired_unix};

use crate::{
    apply_trust_root_mutation_specs, load_trust_root_records, parse_trusted_root_spec,
    save_trust_root_records, TrustedKey,
};

mod io;
mod schema;
mod validation;

use io::{resolve_component_source_bytes, resolve_component_source_path};
use schema::{
    PackageActivationSelection, PackageComponent, PackageManifest, PACKAGE_MANIFEST_SCHEMA_VERSION,
};
use validation::{
    is_semver_like, parse_component_source_url, parse_sha256_checksum, validate_component_set,
    validate_relative_component_path,
};

fn resolve_skill_trust_roots(cli: &Cli) -> Result<Vec<TrustedKey>> {
    let has_store_mutation = !cli.skill_trust_add.is_empty()
        || !cli.skill_trust_revoke.is_empty()
        || !cli.skill_trust_rotate.is_empty();
    if has_store_mutation && cli.skill_trust_root_file.is_none() {
        bail!("--skill-trust-root-file is required when using trust lifecycle flags");
    }

    let mut roots = Vec::new();
    for raw in &cli.skill_trust_root {
        roots.push(parse_trusted_root_spec(raw)?);
    }

    if let Some(path) = &cli.skill_trust_root_file {
        let mut records = load_trust_root_records(path)?;
        if has_store_mutation {
            let report = apply_trust_root_mutation_specs(
                &mut records,
                &cli.skill_trust_add,
                &cli.skill_trust_revoke,
                &cli.skill_trust_rotate,
            )?;
            save_trust_root_records(path, &records)?;
            println!(
                "skill trust store update: added={} updated={} revoked={} rotated={}",
                report.added, report.updated, report.revoked, report.rotated
            );
        }

        let now_unix = current_unix_timestamp();
        for item in records {
            if item.revoked || is_expired_unix(item.expires_unix, now_unix) {
                continue;
            }
            roots.push(TrustedKey {
                id: item.id,
                public_key: item.public_key,
            });
        }
    }

    Ok(roots)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `PackageManifestSummary` used across Tau components.
pub struct PackageManifestSummary {
    pub manifest_path: PathBuf,
    pub name: String,
    pub version: String,
    pub template_count: usize,
    pub skill_count: usize,
    pub extension_count: usize,
    pub theme_count: usize,
    pub total_components: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `PackageInstallReport` used across Tau components.
pub struct PackageInstallReport {
    pub manifest_path: PathBuf,
    pub install_root: PathBuf,
    pub package_dir: PathBuf,
    pub name: String,
    pub version: String,
    pub manifest_status: FileUpsertOutcome,
    pub installed: usize,
    pub updated: usize,
    pub skipped: usize,
    pub total_components: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `PackageListEntry` used across Tau components.
pub struct PackageListEntry {
    pub manifest_path: PathBuf,
    pub package_dir: PathBuf,
    pub name: String,
    pub version: String,
    pub total_components: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `PackageListInvalidEntry` used across Tau components.
pub struct PackageListInvalidEntry {
    pub package_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `PackageListReport` used across Tau components.
pub struct PackageListReport {
    pub list_root: PathBuf,
    pub packages: Vec<PackageListEntry>,
    pub invalid_entries: Vec<PackageListInvalidEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `PackageConflictEntry` used across Tau components.
pub struct PackageConflictEntry {
    pub kind: String,
    pub path: String,
    pub winner: String,
    pub contender: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `PackageConflictReport` used across Tau components.
pub struct PackageConflictReport {
    pub conflict_root: PathBuf,
    pub packages: usize,
    pub invalid_entries: Vec<PackageListInvalidEntry>,
    pub conflicts: Vec<PackageConflictEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `PackageActivationConflictPolicy` values.
pub enum PackageActivationConflictPolicy {
    Error,
    KeepFirst,
    KeepLast,
}

impl PackageActivationConflictPolicy {
    fn parse(raw: &str) -> Result<Self> {
        match raw.trim() {
            "error" => Ok(Self::Error),
            "keep-first" => Ok(Self::KeepFirst),
            "keep-last" => Ok(Self::KeepLast),
            other => bail!(
                "unsupported package activation conflict policy '{}': expected one of error, keep-first, keep-last",
                other
            ),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::KeepFirst => "keep-first",
            Self::KeepLast => "keep-last",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `PackageActivationReport` used across Tau components.
pub struct PackageActivationReport {
    pub activation_root: PathBuf,
    pub destination_root: PathBuf,
    pub policy: PackageActivationConflictPolicy,
    pub packages: usize,
    pub activated_components: usize,
    pub conflicts_detected: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `PackageRemoveReport` used across Tau components.
pub struct PackageRemoveReport {
    pub remove_root: PathBuf,
    pub package_dir: PathBuf,
    pub name: String,
    pub version: String,
    pub status: PackageRemoveStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `PackageRemoveStatus` values.
pub enum PackageRemoveStatus {
    Removed,
    NotFound,
}

impl PackageRemoveStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Removed => "removed",
            Self::NotFound => "not_found",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `PackageRollbackReport` used across Tau components.
pub struct PackageRollbackReport {
    pub rollback_root: PathBuf,
    pub package_name: String,
    pub target_version: String,
    pub removed_versions: Vec<String>,
    pub status: PackageRollbackStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `PackageRollbackStatus` values.
pub enum PackageRollbackStatus {
    RolledBack,
    AlreadyAtTarget,
}

impl PackageRollbackStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::RolledBack => "rolled_back",
            Self::AlreadyAtTarget => "already_at_target",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `FileUpsertOutcome` values.
pub enum FileUpsertOutcome {
    Installed,
    Updated,
    Skipped,
}

impl FileUpsertOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::Updated => "updated",
            Self::Skipped => "skipped",
        }
    }
}

pub fn execute_package_validate_command(cli: &Cli) -> Result<()> {
    let Some(path) = cli.package_validate.as_ref() else {
        return Ok(());
    };
    let summary = validate_package_manifest(path)?;
    println!(
        "package validate: path={} name={} version={} templates={} skills={} extensions={} themes={} total_components={}",
        summary.manifest_path.display(),
        summary.name,
        summary.version,
        summary.template_count,
        summary.skill_count,
        summary.extension_count,
        summary.theme_count,
        summary.total_components,
    );
    Ok(())
}

pub fn execute_package_show_command(cli: &Cli) -> Result<()> {
    let Some(path) = cli.package_show.as_ref() else {
        return Ok(());
    };
    let (manifest, summary) = load_and_validate_manifest(path)?;
    println!("{}", render_package_manifest_report(&summary, &manifest));
    Ok(())
}

pub fn execute_package_install_command(cli: &Cli) -> Result<()> {
    let Some(path) = cli.package_install.as_ref() else {
        return Ok(());
    };
    let trusted_roots = resolve_skill_trust_roots(cli)?;
    let report = install_package_manifest_with_policy(
        path,
        &cli.package_install_root,
        cli.require_signed_packages,
        &trusted_roots,
    )?;
    println!("{}", render_package_install_report(&report));
    Ok(())
}

pub fn execute_package_update_command(cli: &Cli) -> Result<()> {
    let Some(path) = cli.package_update.as_ref() else {
        return Ok(());
    };
    let trusted_roots = resolve_skill_trust_roots(cli)?;
    let report = update_package_manifest_with_policy(
        path,
        &cli.package_update_root,
        cli.require_signed_packages,
        &trusted_roots,
    )?;
    println!("{}", render_package_update_report(&report));
    Ok(())
}

pub fn execute_package_list_command(cli: &Cli) -> Result<()> {
    if !cli.package_list {
        return Ok(());
    }
    let report = list_installed_packages(&cli.package_list_root)?;
    println!("{}", render_package_list_report(&report));
    Ok(())
}

pub fn execute_package_remove_command(cli: &Cli) -> Result<()> {
    let Some(coordinate) = cli.package_remove.as_deref() else {
        return Ok(());
    };
    let report = remove_installed_package(coordinate, &cli.package_remove_root)?;
    println!("{}", render_package_remove_report(&report));
    Ok(())
}

pub fn execute_package_rollback_command(cli: &Cli) -> Result<()> {
    let Some(coordinate) = cli.package_rollback.as_deref() else {
        return Ok(());
    };
    let report = rollback_installed_package(coordinate, &cli.package_rollback_root)?;
    println!("{}", render_package_rollback_report(&report));
    Ok(())
}

pub fn execute_package_conflicts_command(cli: &Cli) -> Result<()> {
    if !cli.package_conflicts {
        return Ok(());
    }
    let report = scan_installed_package_conflicts(&cli.package_conflicts_root)?;
    println!("{}", render_package_conflict_report(&report));
    Ok(())
}

pub fn execute_package_activate_command(cli: &Cli) -> Result<()> {
    if !cli.package_activate {
        return Ok(());
    }
    let report = activate_packages_from_cli(cli)?;
    println!("{}", render_package_activation_report(&report));
    Ok(())
}

pub fn execute_package_activate_on_startup(cli: &Cli) -> Result<Option<PackageActivationReport>> {
    if !cli.package_activate_on_startup {
        return Ok(None);
    }
    let report = activate_packages_from_cli(cli)?;
    println!("{}", render_package_activation_report(&report));
    Ok(Some(report))
}

fn activate_packages_from_cli(cli: &Cli) -> Result<PackageActivationReport> {
    let policy =
        PackageActivationConflictPolicy::parse(cli.package_activate_conflict_policy.as_str())?;
    activate_installed_packages(
        &cli.package_activate_root,
        &cli.package_activate_destination,
        policy,
    )
}

pub fn validate_package_manifest(path: &Path) -> Result<PackageManifestSummary> {
    let (_, summary) = load_and_validate_manifest(path)?;
    Ok(summary)
}

fn load_and_validate_manifest(path: &Path) -> Result<(PackageManifest, PackageManifestSummary)> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read package manifest {}", path.display()))?;
    let manifest = serde_json::from_str::<PackageManifest>(&raw)
        .with_context(|| format!("failed to parse package manifest JSON {}", path.display()))?;
    if manifest.schema_version != PACKAGE_MANIFEST_SCHEMA_VERSION {
        bail!(
            "unsupported package manifest schema: expected {}, found {}",
            PACKAGE_MANIFEST_SCHEMA_VERSION,
            manifest.schema_version
        );
    }
    let name = manifest.name.trim();
    if name.is_empty() {
        bail!("package manifest name must be non-empty");
    }
    if !is_semver_like(manifest.version.trim()) {
        bail!(
            "package manifest version '{}' must follow x.y.z numeric semver form",
            manifest.version
        );
    }
    match (
        manifest.signing_key.as_deref(),
        manifest.signature_file.as_deref(),
    ) {
        (Some(signing_key), Some(signature_file)) => {
            let signing_key = signing_key.trim();
            if signing_key.is_empty() {
                bail!("package manifest signing_key must be non-empty when signature_file is set");
            }
            validate_relative_component_path("signature", signing_key, signature_file.trim())?;
        }
        (None, None) => {}
        _ => bail!(
            "package manifest signing metadata is incomplete: signing_key and signature_file are required together"
        ),
    }

    let mut total_components = 0_usize;
    validate_component_set("templates", &manifest.templates)?;
    total_components = total_components.saturating_add(manifest.templates.len());
    validate_component_set("skills", &manifest.skills)?;
    total_components = total_components.saturating_add(manifest.skills.len());
    validate_component_set("extensions", &manifest.extensions)?;
    total_components = total_components.saturating_add(manifest.extensions.len());
    validate_component_set("themes", &manifest.themes)?;
    total_components = total_components.saturating_add(manifest.themes.len());
    if total_components == 0 {
        bail!("package manifest must declare at least one component");
    }

    let summary = PackageManifestSummary {
        manifest_path: path.to_path_buf(),
        name: name.to_string(),
        version: manifest.version.trim().to_string(),
        template_count: manifest.templates.len(),
        skill_count: manifest.skills.len(),
        extension_count: manifest.extensions.len(),
        theme_count: manifest.themes.len(),
        total_components,
    };
    Ok((manifest, summary))
}

#[cfg(test)]
fn install_package_manifest(
    manifest_path: &Path,
    install_root: &Path,
) -> Result<PackageInstallReport> {
    install_package_manifest_with_policy(manifest_path, install_root, false, &[])
}

fn install_package_manifest_with_policy(
    manifest_path: &Path,
    install_root: &Path,
    require_signed_packages: bool,
    trusted_roots: &[TrustedKey],
) -> Result<PackageInstallReport> {
    let (manifest, summary) = load_and_validate_manifest(manifest_path)?;
    verify_package_signature_policy(
        manifest_path,
        &manifest,
        require_signed_packages,
        trusted_roots,
    )?;
    let manifest_dir = manifest_path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let canonical_manifest_dir = std::fs::canonicalize(manifest_dir).with_context(|| {
        format!(
            "failed to canonicalize package manifest directory {}",
            manifest_dir.display()
        )
    })?;

    std::fs::create_dir_all(install_root)
        .with_context(|| format!("failed to create {}", install_root.display()))?;
    let package_dir = install_root
        .join(summary.name.as_str())
        .join(summary.version.as_str());
    std::fs::create_dir_all(&package_dir)
        .with_context(|| format!("failed to create {}", package_dir.display()))?;

    let manifest_status =
        upsert_file_from_source(manifest_path, &package_dir.join("package.json"))?;
    let mut report = PackageInstallReport {
        manifest_path: manifest_path.to_path_buf(),
        install_root: install_root.to_path_buf(),
        package_dir: package_dir.clone(),
        name: summary.name,
        version: summary.version,
        manifest_status,
        installed: 0,
        updated: 0,
        skipped: 0,
        total_components: summary.total_components,
    };

    install_component_set(
        "templates",
        &manifest.templates,
        &canonical_manifest_dir,
        &package_dir,
        &mut report,
    )?;
    install_component_set(
        "skills",
        &manifest.skills,
        &canonical_manifest_dir,
        &package_dir,
        &mut report,
    )?;
    install_component_set(
        "extensions",
        &manifest.extensions,
        &canonical_manifest_dir,
        &package_dir,
        &mut report,
    )?;
    install_component_set(
        "themes",
        &manifest.themes,
        &canonical_manifest_dir,
        &package_dir,
        &mut report,
    )?;

    Ok(report)
}

fn update_package_manifest_with_policy(
    manifest_path: &Path,
    update_root: &Path,
    require_signed_packages: bool,
    trusted_roots: &[TrustedKey],
) -> Result<PackageInstallReport> {
    let (_, summary) = load_and_validate_manifest(manifest_path)?;
    let package_dir = update_root
        .join(summary.name.as_str())
        .join(summary.version.as_str());
    if !package_dir.is_dir() {
        bail!(
            "package update target '{}@{}' is not installed under {}",
            summary.name,
            summary.version,
            update_root.display()
        );
    }
    install_package_manifest_with_policy(
        manifest_path,
        update_root,
        require_signed_packages,
        trusted_roots,
    )
}

fn verify_package_signature_policy(
    manifest_path: &Path,
    manifest: &PackageManifest,
    require_signed_packages: bool,
    trusted_roots: &[TrustedKey],
) -> Result<()> {
    match (
        manifest.signing_key.as_deref(),
        manifest.signature_file.as_deref(),
    ) {
        (Some(signing_key), Some(signature_file)) => {
            let signing_key = signing_key.trim();
            let trusted_key = trusted_roots
                .iter()
                .find(|key| key.id == signing_key)
                .ok_or_else(|| {
                    anyhow!(
                        "package manifest signing key '{}' is not trusted; configure --skill-trust-root or --skill-trust-root-file",
                        signing_key
                    )
                })?;
            let manifest_dir = manifest_path
                .parent()
                .filter(|dir| !dir.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            let canonical_manifest_dir =
                std::fs::canonicalize(manifest_dir).with_context(|| {
                    format!(
                        "failed to canonicalize package manifest directory {}",
                        manifest_dir.display()
                    )
                })?;
            let signature_path = resolve_component_source_path(
                "signature",
                signing_key,
                Path::new(signature_file.trim()),
                &canonical_manifest_dir,
            )?;
            let signature_base64 = std::fs::read_to_string(&signature_path).with_context(|| {
                format!(
                    "failed to read package signature file {}",
                    signature_path.display()
                )
            })?;
            let signature_base64 = signature_base64.trim();
            if signature_base64.is_empty() {
                bail!(
                    "package manifest signature file '{}' is empty",
                    signature_path.display()
                );
            }
            let manifest_bytes = std::fs::read(manifest_path).with_context(|| {
                format!("failed to read package manifest {}", manifest_path.display())
            })?;
            verify_ed25519_signature(
                &manifest_bytes,
                signature_base64,
                trusted_key.public_key.trim(),
            )
            .with_context(|| {
                format!(
                    "package manifest signature verification failed for key '{}'",
                    signing_key
                )
            })?;
            Ok(())
        }
        (None, None) => {
            if require_signed_packages {
                bail!(
                    "package manifest must include signing_key and signature_file when --require-signed-packages is enabled"
                );
            }
            Ok(())
        }
        _ => bail!(
            "package manifest signing metadata is incomplete: signing_key and signature_file are required together"
        ),
    }
}

fn verify_ed25519_signature(
    message: &[u8],
    signature_base64: &str,
    public_key_base64: &str,
) -> Result<()> {
    let signature_bytes = decode_base64_fixed::<64>("signature", signature_base64)?;
    let public_key_bytes = decode_base64_fixed::<32>("public key", public_key_base64)?;
    let public_key = VerifyingKey::from_bytes(&public_key_bytes)
        .context("failed to decode ed25519 public key bytes")?;
    let signature = Signature::from_bytes(&signature_bytes);
    public_key
        .verify_strict(message, &signature)
        .map_err(|error| anyhow!("invalid ed25519 signature: {error}"))?;
    Ok(())
}

fn decode_base64_fixed<const N: usize>(label: &str, raw: &str) -> Result<[u8; N]> {
    let decoded = BASE64
        .decode(raw.trim())
        .with_context(|| format!("failed to decode {label} from base64"))?;
    decoded.as_slice().try_into().map_err(|_| {
        anyhow!(
            "decoded {label} has invalid length: expected {N} bytes, found {}",
            decoded.len()
        )
    })
}

fn render_package_install_report(report: &PackageInstallReport) -> String {
    render_package_sync_report("package install", report)
}

fn render_package_update_report(report: &PackageInstallReport) -> String {
    render_package_sync_report("package update", report)
}

fn render_package_sync_report(label: &str, report: &PackageInstallReport) -> String {
    format!(
        "{label}: manifest={} root={} package_dir={} name={} version={} manifest_status={} installed={} updated={} skipped={} total_components={}",
        report.manifest_path.display(),
        report.install_root.display(),
        report.package_dir.display(),
        report.name,
        report.version,
        report.manifest_status.as_str(),
        report.installed,
        report.updated,
        report.skipped,
        report.total_components
    )
}

fn install_component_set(
    kind: &str,
    components: &[PackageComponent],
    canonical_manifest_dir: &Path,
    package_dir: &Path,
    report: &mut PackageInstallReport,
) -> Result<()> {
    for component in components {
        let id = component.id.trim();
        let relative_path = PathBuf::from(component.path.trim());
        let source_content = resolve_component_source_bytes(
            kind,
            id,
            component,
            &relative_path,
            canonical_manifest_dir,
        )?;
        let destination = package_dir.join(&relative_path);
        match upsert_file_from_bytes(&source_content, &destination)? {
            FileUpsertOutcome::Installed => report.installed = report.installed.saturating_add(1),
            FileUpsertOutcome::Updated => report.updated = report.updated.saturating_add(1),
            FileUpsertOutcome::Skipped => report.skipped = report.skipped.saturating_add(1),
        }
    }
    Ok(())
}

fn verify_component_checksum(kind: &str, id: &str, raw_checksum: &str, bytes: &[u8]) -> Result<()> {
    let expected_sha256 = parse_sha256_checksum(raw_checksum).with_context(|| {
        format!(
            "package manifest {} entry '{}' has invalid checksum '{}'",
            kind,
            id,
            raw_checksum.trim()
        )
    })?;
    let actual_sha256 = sha256_hex(bytes);
    if expected_sha256 != actual_sha256 {
        bail!(
            "package manifest {} entry '{}' checksum mismatch: expected {}, got {}",
            kind,
            id,
            expected_sha256,
            actual_sha256
        );
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn upsert_file_from_source(source: &Path, destination: &Path) -> Result<FileUpsertOutcome> {
    let source_content = std::fs::read(source)
        .with_context(|| format!("failed to read source file {}", source.display()))?;
    upsert_file_contents(destination, &source_content)
}

fn upsert_file_from_bytes(source_content: &[u8], destination: &Path) -> Result<FileUpsertOutcome> {
    upsert_file_contents(destination, source_content)
}

fn upsert_file_contents(destination: &Path, source_content: &[u8]) -> Result<FileUpsertOutcome> {
    let destination_exists = destination.exists();
    if destination_exists {
        if destination.is_dir() {
            bail!("destination '{}' is a directory", destination.display());
        }
        let existing_content = std::fs::read(destination).with_context(|| {
            format!(
                "failed to read existing destination file {}",
                destination.display()
            )
        })?;
        if existing_content == source_content {
            return Ok(FileUpsertOutcome::Skipped);
        }
    }

    let parent_dir = destination
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent_dir)
        .with_context(|| format!("failed to create {}", parent_dir.display()))?;
    std::fs::write(destination, source_content)
        .with_context(|| format!("failed to write destination file {}", destination.display()))?;
    if destination_exists {
        Ok(FileUpsertOutcome::Updated)
    } else {
        Ok(FileUpsertOutcome::Installed)
    }
}

fn list_installed_packages(list_root: &Path) -> Result<PackageListReport> {
    let mut report = PackageListReport {
        list_root: list_root.to_path_buf(),
        packages: Vec::new(),
        invalid_entries: Vec::new(),
    };
    if !list_root.exists() {
        return Ok(report);
    }
    if !list_root.is_dir() {
        bail!(
            "package list root '{}' is not a directory",
            list_root.display()
        );
    }

    let package_name_dirs = read_sorted_directory_paths(list_root)?;
    for package_name_dir in package_name_dirs {
        if !package_name_dir.is_dir() {
            continue;
        }
        let version_dirs = read_sorted_directory_paths(&package_name_dir)?;
        for package_dir in version_dirs {
            if !package_dir.is_dir() {
                continue;
            }
            let manifest_path = package_dir.join("package.json");
            if !manifest_path.is_file() {
                report.invalid_entries.push(PackageListInvalidEntry {
                    package_dir: package_dir.clone(),
                    manifest_path: manifest_path.clone(),
                    error: "missing package manifest file".to_string(),
                });
                continue;
            }

            match validate_package_manifest(&manifest_path) {
                Ok(summary) => report.packages.push(PackageListEntry {
                    manifest_path,
                    package_dir: package_dir.clone(),
                    name: summary.name,
                    version: summary.version,
                    total_components: summary.total_components,
                }),
                Err(error) => report.invalid_entries.push(PackageListInvalidEntry {
                    package_dir: package_dir.clone(),
                    manifest_path,
                    error: error.to_string(),
                }),
            }
        }
    }

    report.packages.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.version.cmp(&right.version))
            .then_with(|| left.package_dir.cmp(&right.package_dir))
    });
    report.invalid_entries.sort_by(|left, right| {
        left.package_dir
            .cmp(&right.package_dir)
            .then_with(|| left.manifest_path.cmp(&right.manifest_path))
    });
    Ok(report)
}

fn scan_installed_package_conflicts(conflict_root: &Path) -> Result<PackageConflictReport> {
    let list_report = list_installed_packages(conflict_root)?;
    let mut report = PackageConflictReport {
        conflict_root: conflict_root.to_path_buf(),
        packages: list_report.packages.len(),
        invalid_entries: list_report.invalid_entries,
        conflicts: Vec::new(),
    };
    let mut owners: std::collections::BTreeMap<(String, String), String> =
        std::collections::BTreeMap::new();

    for package in list_report.packages {
        let owner = format!("{}@{}", package.name, package.version);
        let manifest_path = package.manifest_path.clone();
        let manifest = match load_and_validate_manifest(&manifest_path) {
            Ok((manifest, _)) => manifest,
            Err(error) => {
                report.invalid_entries.push(PackageListInvalidEntry {
                    package_dir: package.package_dir.clone(),
                    manifest_path,
                    error: error.to_string(),
                });
                continue;
            }
        };
        append_component_conflicts(
            "templates",
            &manifest.templates,
            &owner,
            &mut owners,
            &mut report.conflicts,
        );
        append_component_conflicts(
            "skills",
            &manifest.skills,
            &owner,
            &mut owners,
            &mut report.conflicts,
        );
        append_component_conflicts(
            "extensions",
            &manifest.extensions,
            &owner,
            &mut owners,
            &mut report.conflicts,
        );
        append_component_conflicts(
            "themes",
            &manifest.themes,
            &owner,
            &mut owners,
            &mut report.conflicts,
        );
    }

    report.conflicts.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.winner.cmp(&right.winner))
            .then_with(|| left.contender.cmp(&right.contender))
    });
    report.invalid_entries.sort_by(|left, right| {
        left.package_dir
            .cmp(&right.package_dir)
            .then_with(|| left.manifest_path.cmp(&right.manifest_path))
    });
    Ok(report)
}

fn activate_installed_packages(
    activation_root: &Path,
    destination_root: &Path,
    policy: PackageActivationConflictPolicy,
) -> Result<PackageActivationReport> {
    let list_report = list_installed_packages(activation_root)?;
    if !list_report.invalid_entries.is_empty() {
        bail!(
            "package activation aborted: {} invalid installed package entries found under {}",
            list_report.invalid_entries.len(),
            activation_root.display()
        );
    }

    let package_count = list_report.packages.len();
    let mut selected: std::collections::BTreeMap<(String, String), PackageActivationSelection> =
        std::collections::BTreeMap::new();
    let mut conflicts_detected = 0_usize;

    for package in list_report.packages {
        let owner = format!("{}@{}", package.name, package.version);
        let (manifest, _) = load_and_validate_manifest(&package.manifest_path)?;
        collect_activation_components(
            "templates",
            &manifest.templates,
            &package.package_dir,
            &owner,
            policy,
            &mut selected,
            &mut conflicts_detected,
        )?;
        collect_activation_components(
            "skills",
            &manifest.skills,
            &package.package_dir,
            &owner,
            policy,
            &mut selected,
            &mut conflicts_detected,
        )?;
        collect_activation_components(
            "extensions",
            &manifest.extensions,
            &package.package_dir,
            &owner,
            policy,
            &mut selected,
            &mut conflicts_detected,
        )?;
        collect_activation_components(
            "themes",
            &manifest.themes,
            &package.package_dir,
            &owner,
            policy,
            &mut selected,
            &mut conflicts_detected,
        )?;
    }

    if destination_root.exists() {
        if !destination_root.is_dir() {
            bail!(
                "package activation destination '{}' is not a directory",
                destination_root.display()
            );
        }
        std::fs::remove_dir_all(destination_root).with_context(|| {
            format!(
                "failed to clear package activation destination {}",
                destination_root.display()
            )
        })?;
    }
    std::fs::create_dir_all(destination_root).with_context(|| {
        format!(
            "failed to create package activation destination {}",
            destination_root.display()
        )
    })?;

    for selection in selected.values() {
        let destination =
            resolve_activation_destination_path(destination_root, &selection.kind, &selection.path);
        upsert_file_from_source(&selection.source, &destination)?;
        if let Some(skill_alias_destination) =
            resolve_activation_skill_alias_path(destination_root, &selection.kind, &selection.path)
        {
            upsert_file_from_source(&selection.source, &skill_alias_destination)?;
        }
    }

    Ok(PackageActivationReport {
        activation_root: activation_root.to_path_buf(),
        destination_root: destination_root.to_path_buf(),
        policy,
        packages: package_count,
        activated_components: selected.len(),
        conflicts_detected,
    })
}

fn append_component_conflicts(
    kind: &str,
    components: &[PackageComponent],
    owner: &str,
    owners: &mut std::collections::BTreeMap<(String, String), String>,
    conflicts: &mut Vec<PackageConflictEntry>,
) {
    for component in components {
        let path = component.path.trim().to_string();
        let key = (kind.to_string(), path.clone());
        if let Some(winner) = owners.get(&key) {
            conflicts.push(PackageConflictEntry {
                kind: kind.to_string(),
                path,
                winner: winner.clone(),
                contender: owner.to_string(),
            });
        } else {
            owners.insert(key, owner.to_string());
        }
    }
}

fn collect_activation_components(
    kind: &str,
    components: &[PackageComponent],
    package_dir: &Path,
    owner: &str,
    policy: PackageActivationConflictPolicy,
    selected: &mut std::collections::BTreeMap<(String, String), PackageActivationSelection>,
    conflicts_detected: &mut usize,
) -> Result<()> {
    for component in components {
        let path = component.path.trim().to_string();
        let source = package_dir.join(path.as_str());
        let metadata = std::fs::metadata(&source).with_context(|| {
            format!(
                "activated package component source is missing for {} '{}': {}",
                kind,
                owner,
                source.display()
            )
        })?;
        if !metadata.is_file() {
            bail!(
                "activated package component source for {} '{}' must be a file: {}",
                kind,
                owner,
                source.display()
            );
        }

        let key = (kind.to_string(), path.clone());
        if let Some(existing) = selected.get(&key) {
            *conflicts_detected = conflicts_detected.saturating_add(1);
            match policy {
                PackageActivationConflictPolicy::Error => bail!(
                    "package activation conflict for {} path '{}': {} vs {}",
                    kind,
                    path,
                    existing.owner,
                    owner
                ),
                PackageActivationConflictPolicy::KeepFirst => {}
                PackageActivationConflictPolicy::KeepLast => {
                    selected.insert(
                        key,
                        PackageActivationSelection {
                            kind: kind.to_string(),
                            path,
                            owner: owner.to_string(),
                            source,
                        },
                    );
                }
            }
            continue;
        }
        selected.insert(
            key,
            PackageActivationSelection {
                kind: kind.to_string(),
                path,
                owner: owner.to_string(),
                source,
            },
        );
    }
    Ok(())
}

fn resolve_activation_destination_path(
    destination_root: &Path,
    kind: &str,
    raw_path: &str,
) -> PathBuf {
    let path = Path::new(raw_path);
    let relative = path
        .strip_prefix(kind)
        .ok()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or(path);
    destination_root.join(kind).join(relative)
}

fn resolve_activation_skill_alias_path(
    destination_root: &Path,
    kind: &str,
    raw_path: &str,
) -> Option<PathBuf> {
    if kind != "skills" {
        return None;
    }
    let path = Path::new(raw_path);
    let file_name = path.file_name().and_then(|value| value.to_str())?;
    if file_name != "SKILL.md" {
        return None;
    }
    let relative = path.strip_prefix(kind).ok()?;
    let parent = relative
        .parent()
        .filter(|value| !value.as_os_str().is_empty())?;
    let alias_name = activation_skill_alias_name(parent);
    if alias_name.is_empty() {
        return None;
    }
    Some(
        destination_root
            .join("skills")
            .join(format!("{alias_name}.md")),
    )
}

fn activation_skill_alias_name(path: &Path) -> String {
    let mut alias = String::new();
    for component in path.components() {
        let Component::Normal(segment) = component else {
            continue;
        };
        let segment = segment.to_string_lossy();
        if segment.is_empty() {
            continue;
        }
        if !alias.is_empty() {
            alias.push_str("__");
        }
        for ch in segment.chars() {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                alias.push(ch);
            } else {
                alias.push('_');
            }
        }
    }
    alias
}

fn read_sorted_directory_paths(path: &Path) -> Result<Vec<PathBuf>> {
    let mut entries = Vec::new();
    for entry in
        std::fs::read_dir(path).with_context(|| format!("failed to read {}", path.display()))?
    {
        let entry = entry.with_context(|| format!("failed to read entry in {}", path.display()))?;
        entries.push(entry.path());
    }
    entries.sort();
    Ok(entries)
}

fn render_package_list_report(report: &PackageListReport) -> String {
    let mut lines = vec![format!(
        "package list: root={} packages={} invalid={}",
        report.list_root.display(),
        report.packages.len(),
        report.invalid_entries.len()
    )];
    if report.packages.is_empty() {
        lines.push("packages: none".to_string());
    } else {
        for package in &report.packages {
            lines.push(format!(
                "package: name={} version={} path={} total_components={}",
                package.name,
                package.version,
                package.package_dir.display(),
                package.total_components
            ));
        }
    }

    for invalid in &report.invalid_entries {
        lines.push(format!(
            "package invalid: path={} manifest={} error={}",
            invalid.package_dir.display(),
            invalid.manifest_path.display(),
            invalid.error
        ));
    }
    lines.join("\n")
}

fn render_package_conflict_report(report: &PackageConflictReport) -> String {
    let mut lines = vec![format!(
        "package conflicts: root={} packages={} conflicts={} invalid={}",
        report.conflict_root.display(),
        report.packages,
        report.conflicts.len(),
        report.invalid_entries.len()
    )];
    if report.conflicts.is_empty() {
        lines.push("conflicts: none".to_string());
    } else {
        for conflict in &report.conflicts {
            lines.push(format!(
                "conflict: kind={} path={} winner={} contender={}",
                conflict.kind, conflict.path, conflict.winner, conflict.contender
            ));
        }
    }
    for invalid in &report.invalid_entries {
        lines.push(format!(
            "package invalid: path={} manifest={} error={}",
            invalid.package_dir.display(),
            invalid.manifest_path.display(),
            invalid.error
        ));
    }
    lines.join("\n")
}

fn render_package_activation_report(report: &PackageActivationReport) -> String {
    format!(
        "package activate: root={} destination={} policy={} packages={} activated_components={} conflicts_detected={}",
        report.activation_root.display(),
        report.destination_root.display(),
        report.policy.as_str(),
        report.packages,
        report.activated_components,
        report.conflicts_detected
    )
}

fn remove_installed_package(coordinate: &str, remove_root: &Path) -> Result<PackageRemoveReport> {
    let (name, version) = parse_package_coordinate(coordinate)?;
    let package_dir = remove_root.join(name.as_str()).join(version.as_str());
    if !package_dir.exists() {
        return Ok(PackageRemoveReport {
            remove_root: remove_root.to_path_buf(),
            package_dir,
            name,
            version,
            status: PackageRemoveStatus::NotFound,
        });
    }
    if !package_dir.is_dir() {
        bail!(
            "installed package path '{}' is not a directory",
            package_dir.display()
        );
    }

    std::fs::remove_dir_all(&package_dir).with_context(|| {
        format!(
            "failed to remove installed package {}",
            package_dir.display()
        )
    })?;
    let package_name_dir = remove_root.join(name.as_str());
    if package_name_dir.is_dir() {
        let mut entries = std::fs::read_dir(&package_name_dir)
            .with_context(|| format!("failed to read {}", package_name_dir.display()))?;
        if entries.next().is_none() {
            std::fs::remove_dir(&package_name_dir).with_context(|| {
                format!(
                    "failed to remove empty package directory {}",
                    package_name_dir.display()
                )
            })?;
        }
    }

    Ok(PackageRemoveReport {
        remove_root: remove_root.to_path_buf(),
        package_dir,
        name,
        version,
        status: PackageRemoveStatus::Removed,
    })
}

fn parse_package_coordinate(raw: &str) -> Result<(String, String)> {
    let trimmed = raw.trim();
    let mut parts = trimmed.split('@');
    let Some(name_raw) = parts.next() else {
        bail!("package coordinate must follow <name>@<version>");
    };
    let Some(version_raw) = parts.next() else {
        bail!("package coordinate must follow <name>@<version>");
    };
    if parts.next().is_some() {
        bail!("package coordinate must follow <name>@<version>");
    }

    let name = name_raw.trim();
    if name.is_empty() {
        bail!("package coordinate name must be non-empty");
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        bail!(
            "package coordinate name '{}' must not contain path separators or parent traversals",
            name
        );
    }

    let version = version_raw.trim();
    if !is_semver_like(version) {
        bail!(
            "package coordinate version '{}' must follow x.y.z numeric semver form",
            version
        );
    }
    Ok((name.to_string(), version.to_string()))
}

fn render_package_remove_report(report: &PackageRemoveReport) -> String {
    format!(
        "package remove: root={} name={} version={} path={} status={}",
        report.remove_root.display(),
        report.name,
        report.version,
        report.package_dir.display(),
        report.status.as_str()
    )
}

fn rollback_installed_package(
    coordinate: &str,
    rollback_root: &Path,
) -> Result<PackageRollbackReport> {
    let (package_name, target_version) = parse_package_coordinate(coordinate)?;
    let package_name_dir = rollback_root.join(package_name.as_str());
    let target_dir = package_name_dir.join(target_version.as_str());
    if !target_dir.is_dir() {
        bail!(
            "target package '{}' is not installed under {}",
            coordinate,
            rollback_root.display()
        );
    }

    let mut removed_versions = Vec::new();
    for entry in std::fs::read_dir(&package_name_dir)
        .with_context(|| format!("failed to read {}", package_name_dir.display()))?
    {
        let entry = entry
            .with_context(|| format!("failed to read entry in {}", package_name_dir.display()))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(version) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if version == target_version {
            continue;
        }
        if !is_semver_like(version) {
            continue;
        }
        std::fs::remove_dir_all(&path).with_context(|| {
            format!(
                "failed to remove package version '{}' at {}",
                version,
                path.display()
            )
        })?;
        removed_versions.push(version.to_string());
    }
    removed_versions.sort();
    let status = if removed_versions.is_empty() {
        PackageRollbackStatus::AlreadyAtTarget
    } else {
        PackageRollbackStatus::RolledBack
    };
    Ok(PackageRollbackReport {
        rollback_root: rollback_root.to_path_buf(),
        package_name,
        target_version,
        removed_versions,
        status,
    })
}

fn render_package_rollback_report(report: &PackageRollbackReport) -> String {
    let mut lines = vec![format!(
        "package rollback: root={} name={} target_version={} removed_versions={} status={}",
        report.rollback_root.display(),
        report.package_name,
        report.target_version,
        report.removed_versions.len(),
        report.status.as_str()
    )];
    if report.removed_versions.is_empty() {
        lines.push("rollback removed: none".to_string());
    } else {
        for version in &report.removed_versions {
            lines.push(format!("rollback removed: version={version}"));
        }
    }
    lines.join("\n")
}

fn render_package_manifest_report(
    summary: &PackageManifestSummary,
    manifest: &PackageManifest,
) -> String {
    let mut lines = vec![format!(
        "package show: path={} name={} version={} schema_version={} total_components={}",
        summary.manifest_path.display(),
        summary.name,
        summary.version,
        PACKAGE_MANIFEST_SCHEMA_VERSION,
        summary.total_components
    )];
    append_component_section(&mut lines, "templates", &manifest.templates);
    append_component_section(&mut lines, "skills", &manifest.skills);
    append_component_section(&mut lines, "extensions", &manifest.extensions);
    append_component_section(&mut lines, "themes", &manifest.themes);
    lines.join("\n")
}

fn append_component_section(lines: &mut Vec<String>, label: &str, components: &[PackageComponent]) {
    lines.push(format!("{} ({}):", label, components.len()));
    if components.is_empty() {
        lines.push("none".to_string());
        return;
    }
    for component in components {
        lines.push(format!(
            "- {} => {}",
            component.id.trim(),
            component.path.trim()
        ));
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
    use ed25519_dalek::{Signer, SigningKey};
    use httpmock::prelude::*;
    use sha2::{Digest, Sha256};
    use tempfile::tempdir;

    use crate::TrustedKey;

    use super::{
        activate_installed_packages, install_package_manifest,
        install_package_manifest_with_policy, list_installed_packages, load_and_validate_manifest,
        parse_package_coordinate, remove_installed_package, render_package_activation_report,
        render_package_conflict_report, render_package_install_report, render_package_list_report,
        render_package_manifest_report, render_package_remove_report,
        render_package_rollback_report, render_package_update_report, rollback_installed_package,
        scan_installed_package_conflicts, update_package_manifest_with_policy,
        validate_package_manifest, FileUpsertOutcome, PackageActivationConflictPolicy,
        PackageActivationReport, PackageConflictEntry, PackageConflictReport, PackageListReport,
        PackageRemoveStatus, PackageRollbackStatus,
    };

    #[cfg(unix)]
    fn create_file_symlink(source: &Path, destination: &Path) {
        std::os::unix::fs::symlink(source, destination).expect("create symlink");
    }

    #[cfg(windows)]
    fn create_file_symlink(source: &Path, destination: &Path) {
        std::os::windows::fs::symlink_file(source, destination).expect("create symlink");
    }

    #[cfg(not(any(unix, windows)))]
    fn create_file_symlink(_source: &Path, _destination: &Path) {
        panic!("symlink test requires unix or windows target");
    }

    #[test]
    fn unit_validate_package_manifest_accepts_minimal_semver_shape() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("manifest.json");
        std::fs::write(
            &path,
            r#"{
  "schema_version": 1,
  "name": "starter",
  "version": "1.2.3",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
        )
        .expect("write manifest");

        let summary = validate_package_manifest(&path).expect("validate manifest");
        assert_eq!(summary.name, "starter");
        assert_eq!(summary.version, "1.2.3");
        assert_eq!(summary.template_count, 1);
        assert_eq!(summary.total_components, 1);
    }

    #[test]
    fn functional_validate_package_manifest_counts_components_across_categories() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("manifest.json");
        std::fs::write(
            &path,
            r#"{
  "schema_version": 1,
  "name": "bundle",
  "version": "2.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}],
  "skills": [{"id":"checks","path":"skills/checks/SKILL.md"}],
  "extensions": [{"id":"hooks","path":"extensions/hooks.json"}],
  "themes": [{"id":"solarized","path":"themes/solarized.json"}]
}"#,
        )
        .expect("write manifest");

        let summary = validate_package_manifest(&path).expect("validate manifest");
        assert_eq!(summary.template_count, 1);
        assert_eq!(summary.skill_count, 1);
        assert_eq!(summary.extension_count, 1);
        assert_eq!(summary.theme_count, 1);
        assert_eq!(summary.total_components, 4);
    }

    #[test]
    fn unit_validate_package_manifest_accepts_remote_url_components_with_checksum() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("manifest.json");
        let checksum = format!("{:x}", Sha256::digest(b"remote template"));
        std::fs::write(
            &path,
            format!(
                r#"{{
  "schema_version": 1,
  "name": "bundle",
  "version": "1.0.0",
  "templates": [{{
    "id":"review",
    "path":"templates/review.txt",
    "url":"https://example.com/templates/review.txt",
    "sha256":"sha256:{checksum}"
  }}]
}}"#
            ),
        )
        .expect("write manifest");

        let summary = validate_package_manifest(&path).expect("validate manifest");
        assert_eq!(summary.total_components, 1);
        assert_eq!(summary.template_count, 1);
    }

    #[test]
    fn regression_validate_package_manifest_rejects_duplicate_ids_and_unsafe_paths() {
        let temp = tempdir().expect("tempdir");
        let duplicate_path = temp.path().join("duplicate.json");
        std::fs::write(
            &duplicate_path,
            r#"{
  "schema_version": 1,
  "name": "bundle",
  "version": "1.0.0",
  "templates": [
    {"id":"review","path":"templates/review.txt"},
    {"id":"review","path":"templates/review-alt.txt"}
  ]
}"#,
        )
        .expect("write duplicate manifest");
        let duplicate_error =
            validate_package_manifest(&duplicate_path).expect_err("duplicate ids should fail");
        assert!(duplicate_error
            .to_string()
            .contains("duplicate templates id"));

        let traversal_path = temp.path().join("traversal.json");
        std::fs::write(
            &traversal_path,
            r#"{
  "schema_version": 1,
  "name": "bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"../escape.txt"}]
}"#,
        )
        .expect("write traversal manifest");
        let traversal_error =
            validate_package_manifest(&traversal_path).expect_err("unsafe path should fail");
        assert!(traversal_error
            .to_string()
            .contains("must not contain parent traversals"));
    }

    #[test]
    fn regression_validate_package_manifest_rejects_invalid_remote_url_or_checksum() {
        let temp = tempdir().expect("tempdir");
        let invalid_url_path = temp.path().join("invalid-url.json");
        std::fs::write(
            &invalid_url_path,
            r#"{
  "schema_version": 1,
  "name": "bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt","url":"ftp://example.com/review.txt"}]
}"#,
        )
        .expect("write manifest");
        let url_error = validate_package_manifest(&invalid_url_path)
            .expect_err("unsupported URL scheme should fail");
        assert!(url_error.to_string().contains("must use http or https"));

        let invalid_checksum_path = temp.path().join("invalid-checksum.json");
        std::fs::write(
            &invalid_checksum_path,
            r#"{
  "schema_version": 1,
  "name": "bundle",
  "version": "1.0.0",
  "templates": [{
    "id":"review",
    "path":"templates/review.txt",
    "url":"https://example.com/review.txt",
    "sha256":"sha512:abcd"
  }]
}"#,
        )
        .expect("write manifest");
        let checksum_error = validate_package_manifest(&invalid_checksum_path)
            .expect_err("invalid checksum should fail");
        assert!(checksum_error.to_string().contains("has invalid checksum"));
    }

    #[test]
    fn regression_validate_package_manifest_rejects_invalid_schema_or_version() {
        let temp = tempdir().expect("tempdir");
        let schema_path = temp.path().join("schema.json");
        std::fs::write(
            &schema_path,
            r#"{
  "schema_version": 9,
  "name": "bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
        )
        .expect("write schema manifest");
        let schema_error =
            validate_package_manifest(&schema_path).expect_err("schema mismatch should fail");
        assert!(schema_error
            .to_string()
            .contains("unsupported package manifest schema"));

        let version_path = temp.path().join("version.json");
        std::fs::write(
            &version_path,
            r#"{
  "schema_version": 1,
  "name": "bundle",
  "version": "1.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
        )
        .expect("write version manifest");
        let version_error =
            validate_package_manifest(&version_path).expect_err("invalid version should fail");
        assert!(version_error.to_string().contains("must follow x.y.z"));

        let signature_path = temp.path().join("signature.json");
        std::fs::write(
            &signature_path,
            r#"{
  "schema_version": 1,
  "name": "bundle",
  "version": "1.0.0",
  "signing_key": "publisher",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
        )
        .expect("write signature manifest");
        let signature_error = validate_package_manifest(&signature_path)
            .expect_err("incomplete signing metadata should fail");
        assert!(signature_error
            .to_string()
            .contains("signing metadata is incomplete"));
    }

    #[test]
    fn unit_render_package_manifest_report_includes_category_inventory() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("render.json");
        std::fs::write(
            &path,
            r#"{
  "schema_version": 1,
  "name": "bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}],
  "skills": [{"id":"checks","path":"skills/checks/SKILL.md"}]
}"#,
        )
        .expect("write manifest");

        let (manifest, summary) = load_and_validate_manifest(&path).expect("load manifest");
        let report = render_package_manifest_report(&summary, &manifest);
        assert!(report.contains("package show:"));
        assert!(report.contains("templates (1):"));
        assert!(report.contains("- review => templates/review.txt"));
        assert!(report.contains("skills (1):"));
        assert!(report.contains("extensions (0):"));
        assert!(report.contains("themes (0):"));
    }

    #[test]
    fn functional_install_package_manifest_copies_components_into_versioned_layout() {
        let temp = tempdir().expect("tempdir");
        let package_root = temp.path().join("bundle");
        let templates_dir = package_root.join("templates");
        let skills_dir = package_root.join("skills/checks");
        std::fs::create_dir_all(&templates_dir).expect("create templates dir");
        std::fs::create_dir_all(&skills_dir).expect("create skills dir");
        std::fs::write(templates_dir.join("review.txt"), "template body")
            .expect("write template source");
        std::fs::write(skills_dir.join("SKILL.md"), "# checks").expect("write skill source");

        let manifest_path = package_root.join("package.json");
        std::fs::write(
            &manifest_path,
            r#"{
  "schema_version": 1,
  "name": "starter",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}],
  "skills": [{"id":"checks","path":"skills/checks/SKILL.md"}]
}"#,
        )
        .expect("write manifest");

        let install_root = temp.path().join("installed");
        let first =
            install_package_manifest(&manifest_path, &install_root).expect("install package");
        assert_eq!(first.name, "starter");
        assert_eq!(first.version, "1.0.0");
        assert_eq!(first.total_components, 2);
        assert_eq!(first.installed, 2);
        assert_eq!(first.updated, 0);
        assert_eq!(first.skipped, 0);
        assert_eq!(first.manifest_status, FileUpsertOutcome::Installed);
        assert_eq!(
            std::fs::read_to_string(install_root.join("starter/1.0.0/templates/review.txt"))
                .expect("read installed template"),
            "template body"
        );
        assert_eq!(
            std::fs::read_to_string(install_root.join("starter/1.0.0/skills/checks/SKILL.md"))
                .expect("read installed skill"),
            "# checks"
        );

        let second =
            install_package_manifest(&manifest_path, &install_root).expect("reinstall package");
        assert_eq!(second.installed, 0);
        assert_eq!(second.updated, 0);
        assert_eq!(second.skipped, 2);
        assert_eq!(second.manifest_status, FileUpsertOutcome::Skipped);
    }

    #[test]
    fn functional_install_package_manifest_downloads_remote_component_with_checksum() {
        let server = MockServer::start();
        let remote_body = "remote template body";
        let remote_mock = server.mock(|when, then| {
            when.method(GET).path("/templates/review.txt");
            then.status(200).body(remote_body);
        });

        let temp = tempdir().expect("tempdir");
        let package_root = temp.path().join("bundle");
        std::fs::create_dir_all(&package_root).expect("create bundle dir");
        let checksum = format!("{:x}", Sha256::digest(remote_body.as_bytes()));
        let manifest_path = package_root.join("package.json");
        std::fs::write(
            &manifest_path,
            format!(
                r#"{{
  "schema_version": 1,
  "name": "starter",
  "version": "1.0.0",
  "templates": [{{
    "id":"review",
    "path":"templates/review.txt",
    "url":"{}/templates/review.txt",
    "sha256":"sha256:{checksum}"
  }}]
}}"#,
                server.base_url()
            ),
        )
        .expect("write manifest");

        let install_root = temp.path().join("installed");
        let report =
            install_package_manifest(&manifest_path, &install_root).expect("install package");
        assert_eq!(report.installed, 1);
        assert_eq!(report.updated, 0);
        assert_eq!(report.skipped, 0);
        assert_eq!(
            std::fs::read_to_string(install_root.join("starter/1.0.0/templates/review.txt"))
                .expect("read installed file"),
            remote_body
        );
        remote_mock.assert();
    }

    #[test]
    fn functional_update_package_manifest_with_policy_updates_existing_bundle() {
        let temp = tempdir().expect("tempdir");
        let package_root = temp.path().join("bundle");
        std::fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
        let template_path = package_root.join("templates/review.txt");
        std::fs::write(&template_path, "v1 template").expect("write template source");
        let manifest_path = package_root.join("package.json");
        std::fs::write(
            &manifest_path,
            r#"{
  "schema_version": 1,
  "name": "starter",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
        )
        .expect("write manifest");

        let install_root = temp.path().join("installed");
        install_package_manifest(&manifest_path, &install_root).expect("install package");
        std::fs::write(&template_path, "v2 template").expect("update template source");

        let report = update_package_manifest_with_policy(&manifest_path, &install_root, false, &[])
            .expect("package update should succeed");
        assert_eq!(report.installed, 0);
        assert_eq!(report.updated, 1);
        assert_eq!(report.skipped, 0);
        assert_eq!(
            std::fs::read_to_string(install_root.join("starter/1.0.0/templates/review.txt"))
                .expect("read updated template"),
            "v2 template"
        );
    }

    #[test]
    fn functional_install_package_manifest_with_policy_verifies_signature() {
        let temp = tempdir().expect("tempdir");
        let package_root = temp.path().join("bundle");
        std::fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
        std::fs::write(package_root.join("templates/review.txt"), "template body")
            .expect("write template source");
        let manifest_path = package_root.join("package.json");
        std::fs::write(
            &manifest_path,
            r#"{
  "schema_version": 1,
  "name": "starter",
  "version": "1.0.0",
  "signing_key": "publisher",
  "signature_file": "package.sig",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
        )
        .expect("write manifest");

        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let signature = signing_key.sign(&std::fs::read(&manifest_path).expect("read manifest"));
        std::fs::write(
            package_root.join("package.sig"),
            BASE64.encode(signature.to_bytes()),
        )
        .expect("write signature");
        let trusted_roots = vec![TrustedKey {
            id: "publisher".to_string(),
            public_key: BASE64.encode(signing_key.verifying_key().as_bytes()),
        }];

        let install_root = temp.path().join("installed");
        let report = install_package_manifest_with_policy(
            &manifest_path,
            &install_root,
            true,
            &trusted_roots,
        )
        .expect("signed install should succeed");
        assert_eq!(report.installed, 1);
        assert_eq!(report.total_components, 1);
    }

    #[test]
    fn regression_install_package_manifest_with_policy_rejects_unsigned_when_required() {
        let temp = tempdir().expect("tempdir");
        let package_root = temp.path().join("bundle");
        std::fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
        std::fs::write(package_root.join("templates/review.txt"), "template body")
            .expect("write template source");
        let manifest_path = package_root.join("package.json");
        std::fs::write(
            &manifest_path,
            r#"{
  "schema_version": 1,
  "name": "starter",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
        )
        .expect("write manifest");

        let install_root = temp.path().join("installed");
        let error = install_package_manifest_with_policy(&manifest_path, &install_root, true, &[])
            .expect_err("unsigned package should fail when signatures are required");
        assert!(error
            .to_string()
            .contains("must include signing_key and signature_file"));
    }

    #[test]
    fn regression_install_package_manifest_with_policy_rejects_invalid_signature() {
        let temp = tempdir().expect("tempdir");
        let package_root = temp.path().join("bundle");
        std::fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
        std::fs::write(package_root.join("templates/review.txt"), "template body")
            .expect("write template source");
        let manifest_path = package_root.join("package.json");
        std::fs::write(
            &manifest_path,
            r#"{
  "schema_version": 1,
  "name": "starter",
  "version": "1.0.0",
  "signing_key": "publisher",
  "signature_file": "package.sig",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
        )
        .expect("write manifest");
        std::fs::write(package_root.join("package.sig"), "invalid-signature")
            .expect("write signature");

        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let trusted_roots = vec![TrustedKey {
            id: "publisher".to_string(),
            public_key: BASE64.encode(signing_key.verifying_key().as_bytes()),
        }];
        let install_root = temp.path().join("installed");
        let error = install_package_manifest_with_policy(
            &manifest_path,
            &install_root,
            true,
            &trusted_roots,
        )
        .expect_err("invalid signature should fail");
        assert!(error.to_string().contains("signature verification failed"));
    }

    #[test]
    fn integration_update_package_manifest_with_policy_verifies_signature() {
        let temp = tempdir().expect("tempdir");
        let package_root = temp.path().join("bundle");
        std::fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
        let template_path = package_root.join("templates/review.txt");
        std::fs::write(&template_path, "v1 template").expect("write template");
        let manifest_path = package_root.join("package.json");
        std::fs::write(
            &manifest_path,
            r#"{
  "schema_version": 1,
  "name": "starter",
  "version": "1.0.0",
  "signing_key": "publisher",
  "signature_file": "package.sig",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
        )
        .expect("write manifest");

        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let write_signature = || {
            let signature =
                signing_key.sign(&std::fs::read(&manifest_path).expect("read manifest bytes"));
            std::fs::write(
                package_root.join("package.sig"),
                BASE64.encode(signature.to_bytes()),
            )
            .expect("write signature");
        };
        write_signature();
        let trusted_roots = vec![TrustedKey {
            id: "publisher".to_string(),
            public_key: BASE64.encode(signing_key.verifying_key().as_bytes()),
        }];

        let install_root = temp.path().join("installed");
        install_package_manifest_with_policy(&manifest_path, &install_root, true, &trusted_roots)
            .expect("install signed package");

        std::fs::write(&template_path, "v2 template").expect("update template source");
        write_signature();
        let report = update_package_manifest_with_policy(
            &manifest_path,
            &install_root,
            true,
            &trusted_roots,
        )
        .expect("signed update should succeed");
        assert_eq!(report.updated, 1);
        assert_eq!(
            std::fs::read_to_string(install_root.join("starter/1.0.0/templates/review.txt"))
                .expect("read updated template"),
            "v2 template"
        );
    }

    #[test]
    fn integration_install_package_manifest_with_policy_verifies_signed_remote_components() {
        let server = MockServer::start();
        let remote_body = "remote signed template";
        let remote_mock = server.mock(|when, then| {
            when.method(GET).path("/templates/review.txt");
            then.status(200).body(remote_body);
        });

        let temp = tempdir().expect("tempdir");
        let package_root = temp.path().join("bundle");
        std::fs::create_dir_all(&package_root).expect("create bundle dir");
        let checksum = format!("{:x}", Sha256::digest(remote_body.as_bytes()));
        let manifest_path = package_root.join("package.json");
        std::fs::write(
            &manifest_path,
            format!(
                r#"{{
  "schema_version": 1,
  "name": "starter",
  "version": "1.0.0",
  "signing_key": "publisher",
  "signature_file": "package.sig",
  "templates": [{{
    "id":"review",
    "path":"templates/review.txt",
    "url":"{}/templates/review.txt",
    "sha256":"sha256:{checksum}"
  }}]
}}"#,
                server.base_url()
            ),
        )
        .expect("write manifest");

        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let signature = signing_key.sign(&std::fs::read(&manifest_path).expect("read manifest"));
        std::fs::write(
            package_root.join("package.sig"),
            BASE64.encode(signature.to_bytes()),
        )
        .expect("write signature");
        let trusted_roots = vec![TrustedKey {
            id: "publisher".to_string(),
            public_key: BASE64.encode(signing_key.verifying_key().as_bytes()),
        }];

        let install_root = temp.path().join("installed");
        let report = install_package_manifest_with_policy(
            &manifest_path,
            &install_root,
            true,
            &trusted_roots,
        )
        .expect("signed remote install should succeed");
        assert_eq!(report.installed, 1);
        assert_eq!(
            std::fs::read_to_string(install_root.join("starter/1.0.0/templates/review.txt"))
                .expect("read installed template"),
            remote_body
        );
        remote_mock.assert();
    }

    #[test]
    fn regression_install_package_manifest_with_policy_rejects_untrusted_signing_key() {
        let temp = tempdir().expect("tempdir");
        let package_root = temp.path().join("bundle");
        std::fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
        std::fs::write(package_root.join("templates/review.txt"), "template body")
            .expect("write template source");
        let manifest_path = package_root.join("package.json");
        std::fs::write(
            &manifest_path,
            r#"{
  "schema_version": 1,
  "name": "starter",
  "version": "1.0.0",
  "signing_key": "publisher",
  "signature_file": "package.sig",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
        )
        .expect("write manifest");

        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let signature = signing_key.sign(&std::fs::read(&manifest_path).expect("read manifest"));
        std::fs::write(
            package_root.join("package.sig"),
            BASE64.encode(signature.to_bytes()),
        )
        .expect("write signature");
        let trusted_roots = vec![TrustedKey {
            id: "different-key".to_string(),
            public_key: BASE64.encode(signing_key.verifying_key().as_bytes()),
        }];
        let install_root = temp.path().join("installed");
        let error = install_package_manifest_with_policy(
            &manifest_path,
            &install_root,
            true,
            &trusted_roots,
        )
        .expect_err("untrusted signing key should fail");
        assert!(error.to_string().contains("is not trusted"));
    }

    #[test]
    fn regression_install_package_manifest_with_policy_rejects_unsigned_remote_manifest_when_required(
    ) {
        let server = MockServer::start();
        let remote_body = "unsigned remote template";
        let remote_mock = server.mock(|when, then| {
            when.method(GET).path("/templates/review.txt");
            then.status(200).body(remote_body);
        });

        let temp = tempdir().expect("tempdir");
        let package_root = temp.path().join("bundle");
        std::fs::create_dir_all(&package_root).expect("create bundle dir");
        let checksum = format!("{:x}", Sha256::digest(remote_body.as_bytes()));
        let manifest_path = package_root.join("package.json");
        std::fs::write(
            &manifest_path,
            format!(
                r#"{{
  "schema_version": 1,
  "name": "starter",
  "version": "1.0.0",
  "templates": [{{
    "id":"review",
    "path":"templates/review.txt",
    "url":"{}/templates/review.txt",
    "sha256":"sha256:{checksum}"
  }}]
}}"#,
                server.base_url()
            ),
        )
        .expect("write manifest");

        let install_root = temp.path().join("installed");
        let error = install_package_manifest_with_policy(&manifest_path, &install_root, true, &[])
            .expect_err("unsigned remote package should fail when signatures are required");
        assert!(error
            .to_string()
            .contains("must include signing_key and signature_file"));
        remote_mock.assert_calls(0);
    }

    #[test]
    fn regression_update_package_manifest_with_policy_rejects_missing_target() {
        let temp = tempdir().expect("tempdir");
        let package_root = temp.path().join("bundle");
        std::fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
        std::fs::write(package_root.join("templates/review.txt"), "template")
            .expect("write template source");
        let manifest_path = package_root.join("package.json");
        std::fs::write(
            &manifest_path,
            r#"{
  "schema_version": 1,
  "name": "starter",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
        )
        .expect("write manifest");

        let install_root = temp.path().join("installed");
        let error = update_package_manifest_with_policy(&manifest_path, &install_root, false, &[])
            .expect_err("missing update target should fail");
        assert!(error.to_string().contains("is not installed"));
    }

    #[test]
    fn regression_update_package_manifest_with_policy_rejects_unsigned_when_required() {
        let temp = tempdir().expect("tempdir");
        let package_root = temp.path().join("bundle");
        std::fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
        let template_path = package_root.join("templates/review.txt");
        std::fs::write(&template_path, "template").expect("write template source");
        let manifest_path = package_root.join("package.json");
        std::fs::write(
            &manifest_path,
            r#"{
  "schema_version": 1,
  "name": "starter",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
        )
        .expect("write manifest");

        let install_root = temp.path().join("installed");
        install_package_manifest(&manifest_path, &install_root).expect("install package");
        std::fs::write(&template_path, "template v2").expect("update template");

        let error = update_package_manifest_with_policy(&manifest_path, &install_root, true, &[])
            .expect_err("unsigned update should fail policy");
        assert!(error
            .to_string()
            .contains("must include signing_key and signature_file"));
    }

    #[test]
    fn regression_install_package_manifest_rejects_missing_component_source() {
        let temp = tempdir().expect("tempdir");
        let package_root = temp.path().join("bundle");
        std::fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
        let manifest_path = package_root.join("package.json");
        std::fs::write(
            &manifest_path,
            r#"{
  "schema_version": 1,
  "name": "starter",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/missing.txt"}]
}"#,
        )
        .expect("write manifest");

        let install_root = temp.path().join("installed");
        let error = install_package_manifest(&manifest_path, &install_root)
            .expect_err("missing source should fail");
        assert!(error.to_string().contains("does not exist"));
    }

    #[test]
    fn regression_install_package_manifest_rejects_remote_checksum_mismatch() {
        let server = MockServer::start();
        let remote_mock = server.mock(|when, then| {
            when.method(GET).path("/templates/review.txt");
            then.status(200).body("remote template");
        });

        let temp = tempdir().expect("tempdir");
        let package_root = temp.path().join("bundle");
        std::fs::create_dir_all(&package_root).expect("create bundle dir");
        let manifest_path = package_root.join("package.json");
        std::fs::write(
            &manifest_path,
            format!(
                r#"{{
  "schema_version": 1,
  "name": "starter",
  "version": "1.0.0",
  "templates": [{{
    "id":"review",
    "path":"templates/review.txt",
    "url":"{}/templates/review.txt",
    "sha256":"sha256:{}"
  }}]
}}"#,
                server.base_url(),
                "0".repeat(64)
            ),
        )
        .expect("write manifest");

        let install_root = temp.path().join("installed");
        let error = install_package_manifest(&manifest_path, &install_root)
            .expect_err("checksum mismatch should fail");
        assert!(error.to_string().contains("checksum mismatch"));
        remote_mock.assert();
    }

    #[test]
    fn regression_install_package_manifest_rejects_symlink_escape() {
        let temp = tempdir().expect("tempdir");
        let package_root = temp.path().join("bundle");
        let templates_dir = package_root.join("templates");
        std::fs::create_dir_all(&templates_dir).expect("create templates dir");

        let outside_dir = temp.path().join("outside");
        std::fs::create_dir_all(&outside_dir).expect("create outside dir");
        let outside_file = outside_dir.join("secret.txt");
        std::fs::write(&outside_file, "outside").expect("write outside file");
        create_file_symlink(&outside_file, &templates_dir.join("escape.txt"));

        let manifest_path = package_root.join("package.json");
        std::fs::write(
            &manifest_path,
            r#"{
  "schema_version": 1,
  "name": "starter",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/escape.txt"}]
}"#,
        )
        .expect("write manifest");

        let install_root = temp.path().join("installed");
        let error = install_package_manifest(&manifest_path, &install_root)
            .expect_err("symlink escape should fail");
        assert!(error
            .to_string()
            .contains("resolves outside package manifest directory"));
    }

    #[test]
    fn unit_render_package_install_report_includes_status_and_counts() {
        let report = super::PackageInstallReport {
            manifest_path: Path::new("/tmp/source/package.json").to_path_buf(),
            install_root: Path::new("/tmp/install").to_path_buf(),
            package_dir: Path::new("/tmp/install/starter/1.0.0").to_path_buf(),
            name: "starter".to_string(),
            version: "1.0.0".to_string(),
            manifest_status: FileUpsertOutcome::Updated,
            installed: 1,
            updated: 2,
            skipped: 3,
            total_components: 6,
        };
        let rendered = render_package_install_report(&report);
        assert!(rendered.contains("package install:"));
        assert!(rendered.contains("manifest_status=updated"));
        assert!(rendered.contains("installed=1"));
        assert!(rendered.contains("updated=2"));
        assert!(rendered.contains("skipped=3"));
        assert!(rendered.contains("total_components=6"));
    }

    #[test]
    fn unit_render_package_update_report_includes_status_and_counts() {
        let report = super::PackageInstallReport {
            manifest_path: Path::new("/tmp/source/package.json").to_path_buf(),
            install_root: Path::new("/tmp/install").to_path_buf(),
            package_dir: Path::new("/tmp/install/starter/1.0.0").to_path_buf(),
            name: "starter".to_string(),
            version: "1.0.0".to_string(),
            manifest_status: FileUpsertOutcome::Skipped,
            installed: 0,
            updated: 2,
            skipped: 4,
            total_components: 6,
        };
        let rendered = render_package_update_report(&report);
        assert!(rendered.contains("package update:"));
        assert!(rendered.contains("manifest_status=skipped"));
        assert!(rendered.contains("updated=2"));
        assert!(rendered.contains("skipped=4"));
    }

    #[test]
    fn unit_render_package_conflict_report_includes_summary_details_and_invalid_entries() {
        let report = PackageConflictReport {
            conflict_root: Path::new("/tmp/packages").to_path_buf(),
            packages: 2,
            invalid_entries: vec![super::PackageListInvalidEntry {
                package_dir: Path::new("/tmp/packages/broken/1.0.0").to_path_buf(),
                manifest_path: Path::new("/tmp/packages/broken/1.0.0/package.json").to_path_buf(),
                error: "invalid manifest".to_string(),
            }],
            conflicts: vec![PackageConflictEntry {
                kind: "templates".to_string(),
                path: "templates/review.txt".to_string(),
                winner: "alpha@1.0.0".to_string(),
                contender: "zeta@1.0.0".to_string(),
            }],
        };
        let rendered = render_package_conflict_report(&report);
        assert!(rendered.contains("package conflicts:"));
        assert!(rendered.contains("packages=2"));
        assert!(rendered.contains("conflicts=1"));
        assert!(rendered.contains(
            "conflict: kind=templates path=templates/review.txt winner=alpha@1.0.0 contender=zeta@1.0.0"
        ));
        assert!(rendered.contains("package invalid: path=/tmp/packages/broken/1.0.0"));
    }

    #[test]
    fn functional_scan_installed_package_conflicts_detects_component_collisions() {
        let temp = tempdir().expect("tempdir");
        let install_root = temp.path().join("installed");
        let install_package = |name: &str, body: &str| {
            let source_root = temp.path().join(format!("source-{name}"));
            std::fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
            std::fs::write(source_root.join("templates/review.txt"), body)
                .expect("write template source");
            let manifest_path = source_root.join("package.json");
            std::fs::write(
                &manifest_path,
                format!(
                    r#"{{
  "schema_version": 1,
  "name": "{name}",
  "version": "1.0.0",
  "templates": [{{"id":"review","path":"templates/review.txt"}}]
}}"#
                ),
            )
            .expect("write manifest");
            install_package_manifest(&manifest_path, &install_root).expect("install package");
        };

        install_package("alpha", "alpha body");
        install_package("zeta", "zeta body");

        let report = scan_installed_package_conflicts(&install_root).expect("scan conflicts");
        assert_eq!(report.packages, 2);
        assert_eq!(report.conflicts.len(), 1);
        assert_eq!(report.conflicts[0].kind, "templates");
        assert_eq!(report.conflicts[0].path, "templates/review.txt");
        assert_eq!(report.conflicts[0].winner, "alpha@1.0.0");
        assert_eq!(report.conflicts[0].contender, "zeta@1.0.0");
    }

    #[test]
    fn regression_scan_installed_package_conflicts_reports_none_when_no_collisions_exist() {
        let temp = tempdir().expect("tempdir");
        let install_root = temp.path().join("installed");
        let install_package = |name: &str, path: &str| {
            let source_root = temp.path().join(format!("source-{name}"));
            let component_dir =
                source_root.join(Path::new(path).parent().expect("component parent"));
            std::fs::create_dir_all(&component_dir).expect("create component dir");
            std::fs::write(source_root.join(path), format!("{name} body"))
                .expect("write component source");
            let manifest_path = source_root.join("package.json");
            std::fs::write(
                &manifest_path,
                format!(
                    r#"{{
  "schema_version": 1,
  "name": "{name}",
  "version": "1.0.0",
  "templates": [{{"id":"review","path":"{path}"}}]
}}"#
                ),
            )
            .expect("write manifest");
            install_package_manifest(&manifest_path, &install_root).expect("install package");
        };

        install_package("alpha", "templates/review-a.txt");
        install_package("zeta", "templates/review-z.txt");

        let report = scan_installed_package_conflicts(&install_root).expect("scan conflicts");
        assert_eq!(report.conflicts.len(), 0);
        assert_eq!(report.invalid_entries.len(), 0);
        let rendered = render_package_conflict_report(&report);
        assert!(rendered.contains("conflicts: none"));
    }

    #[test]
    fn regression_scan_installed_package_conflicts_surfaces_invalid_manifest_entries() {
        let temp = tempdir().expect("tempdir");
        let install_root = temp.path().join("installed");

        let source_root = temp.path().join("valid-source");
        std::fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
        std::fs::write(source_root.join("templates/review.txt"), "valid")
            .expect("write template source");
        let valid_manifest = source_root.join("package.json");
        std::fs::write(
            &valid_manifest,
            r#"{
  "schema_version": 1,
  "name": "valid",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
        )
        .expect("write manifest");
        install_package_manifest(&valid_manifest, &install_root).expect("install package");

        let invalid_dir = install_root.join("broken/1.0.0");
        std::fs::create_dir_all(&invalid_dir).expect("create invalid dir");
        std::fs::write(
            invalid_dir.join("package.json"),
            r#"{
  "schema_version": 99,
  "name": "broken",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
        )
        .expect("write invalid manifest");

        let report = scan_installed_package_conflicts(&install_root).expect("scan conflicts");
        assert_eq!(report.packages, 1);
        assert_eq!(report.invalid_entries.len(), 1);
        assert_eq!(report.conflicts.len(), 0);
    }

    #[test]
    fn unit_package_activation_conflict_policy_parses_supported_values() {
        assert_eq!(
            PackageActivationConflictPolicy::parse("error").expect("parse error"),
            PackageActivationConflictPolicy::Error
        );
        assert_eq!(
            PackageActivationConflictPolicy::parse("keep-first").expect("parse keep-first"),
            PackageActivationConflictPolicy::KeepFirst
        );
        assert_eq!(
            PackageActivationConflictPolicy::parse("keep-last").expect("parse keep-last"),
            PackageActivationConflictPolicy::KeepLast
        );

        let error = PackageActivationConflictPolicy::parse("unknown")
            .expect_err("unsupported policy should fail");
        assert!(error
            .to_string()
            .contains("unsupported package activation conflict policy"));
    }

    #[test]
    fn unit_render_package_activation_report_includes_summary_fields() {
        let report = PackageActivationReport {
            activation_root: Path::new("/tmp/packages").to_path_buf(),
            destination_root: Path::new("/tmp/packages-active").to_path_buf(),
            policy: PackageActivationConflictPolicy::KeepFirst,
            packages: 3,
            activated_components: 12,
            conflicts_detected: 4,
        };
        let rendered = render_package_activation_report(&report);
        assert!(rendered.contains("package activate:"));
        assert!(rendered.contains("policy=keep-first"));
        assert!(rendered.contains("packages=3"));
        assert!(rendered.contains("activated_components=12"));
        assert!(rendered.contains("conflicts_detected=4"));
    }

    #[test]
    fn functional_activate_installed_packages_keep_first_materializes_resolved_outputs() {
        let temp = tempdir().expect("tempdir");
        let install_root = temp.path().join("installed");
        let install_package = |name: &str, template: &str, skill: &str| {
            let source_root = temp.path().join(format!("source-{name}"));
            std::fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
            std::fs::create_dir_all(source_root.join("skills/checks")).expect("create skills dir");
            std::fs::write(source_root.join("templates/review.txt"), template)
                .expect("write template source");
            std::fs::write(source_root.join("skills/checks/SKILL.md"), skill)
                .expect("write skill source");
            let manifest_path = source_root.join("package.json");
            std::fs::write(
                &manifest_path,
                format!(
                    r#"{{
  "schema_version": 1,
  "name": "{name}",
  "version": "1.0.0",
  "templates": [{{"id":"review","path":"templates/review.txt"}}],
  "skills": [{{"id":"checks","path":"skills/checks/SKILL.md"}}]
}}"#
                ),
            )
            .expect("write manifest");
            install_package_manifest(&manifest_path, &install_root).expect("install package");
        };

        install_package("alpha", "alpha template", "# alpha");
        install_package("zeta", "zeta template", "# zeta");

        let destination_root = temp.path().join("activated");
        let report = activate_installed_packages(
            &install_root,
            &destination_root,
            PackageActivationConflictPolicy::KeepFirst,
        )
        .expect("activate packages");
        assert_eq!(report.packages, 2);
        assert_eq!(report.activated_components, 2);
        assert_eq!(report.conflicts_detected, 2);
        assert_eq!(
            std::fs::read_to_string(destination_root.join("templates/review.txt"))
                .expect("read activated template"),
            "alpha template"
        );
        assert_eq!(
            std::fs::read_to_string(destination_root.join("skills/checks/SKILL.md"))
                .expect("read activated skill"),
            "# alpha"
        );
    }

    #[test]
    fn functional_activate_installed_packages_keep_last_overwrites_previous_owner() {
        let temp = tempdir().expect("tempdir");
        let install_root = temp.path().join("installed");
        let install_package = |name: &str, template: &str| {
            let source_root = temp.path().join(format!("source-{name}"));
            std::fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
            std::fs::write(source_root.join("templates/review.txt"), template)
                .expect("write template source");
            let manifest_path = source_root.join("package.json");
            std::fs::write(
                &manifest_path,
                format!(
                    r#"{{
  "schema_version": 1,
  "name": "{name}",
  "version": "1.0.0",
  "templates": [{{"id":"review","path":"templates/review.txt"}}]
}}"#
                ),
            )
            .expect("write manifest");
            install_package_manifest(&manifest_path, &install_root).expect("install package");
        };

        install_package("alpha", "alpha template");
        install_package("zeta", "zeta template");

        let destination_root = temp.path().join("activated");
        let report = activate_installed_packages(
            &install_root,
            &destination_root,
            PackageActivationConflictPolicy::KeepLast,
        )
        .expect("activate packages");
        assert_eq!(report.packages, 2);
        assert_eq!(report.activated_components, 1);
        assert_eq!(report.conflicts_detected, 1);
        assert_eq!(
            std::fs::read_to_string(destination_root.join("templates/review.txt"))
                .expect("read activated template"),
            "zeta template"
        );
    }

    #[test]
    fn regression_activate_installed_packages_error_policy_rejects_conflicts() {
        let temp = tempdir().expect("tempdir");
        let install_root = temp.path().join("installed");
        let install_package = |name: &str| {
            let source_root = temp.path().join(format!("source-{name}"));
            std::fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
            std::fs::write(
                source_root.join("templates/review.txt"),
                format!("{name} template"),
            )
            .expect("write template source");
            let manifest_path = source_root.join("package.json");
            std::fs::write(
                &manifest_path,
                format!(
                    r#"{{
  "schema_version": 1,
  "name": "{name}",
  "version": "1.0.0",
  "templates": [{{"id":"review","path":"templates/review.txt"}}]
}}"#
                ),
            )
            .expect("write manifest");
            install_package_manifest(&manifest_path, &install_root).expect("install package");
        };

        install_package("alpha");
        install_package("zeta");

        let error = activate_installed_packages(
            &install_root,
            &temp.path().join("activated"),
            PackageActivationConflictPolicy::Error,
        )
        .expect_err("error policy should fail on conflicts");
        assert!(error.to_string().contains("package activation conflict"));
    }

    #[test]
    fn regression_activate_installed_packages_rejects_invalid_manifest_entries() {
        let temp = tempdir().expect("tempdir");
        let install_root = temp.path().join("installed");

        let source_root = temp.path().join("valid-source");
        std::fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
        std::fs::write(source_root.join("templates/review.txt"), "valid")
            .expect("write template source");
        let valid_manifest = source_root.join("package.json");
        std::fs::write(
            &valid_manifest,
            r#"{
  "schema_version": 1,
  "name": "valid",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
        )
        .expect("write manifest");
        install_package_manifest(&valid_manifest, &install_root).expect("install package");

        let invalid_dir = install_root.join("broken/1.0.0");
        std::fs::create_dir_all(&invalid_dir).expect("create invalid dir");
        std::fs::write(
            invalid_dir.join("package.json"),
            r#"{
  "schema_version": 99,
  "name": "broken",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
        )
        .expect("write invalid manifest");

        let error = activate_installed_packages(
            &install_root,
            &temp.path().join("activated"),
            PackageActivationConflictPolicy::KeepFirst,
        )
        .expect_err("invalid installed entry should fail activation");
        assert!(error
            .to_string()
            .contains("invalid installed package entries"));
    }

    #[test]
    fn functional_list_installed_packages_reports_sorted_inventory() {
        let temp = tempdir().expect("tempdir");
        let install_root = temp.path().join("installed");
        let build_and_install = |name: &str, version: &str, body: &str| {
            let source_root = temp.path().join(format!("source-{name}-{version}"));
            std::fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
            std::fs::write(source_root.join("templates/review.txt"), body)
                .expect("write template source");
            let manifest_path = source_root.join("package.json");
            std::fs::write(
                &manifest_path,
                format!(
                    r#"{{
  "schema_version": 1,
  "name": "{name}",
  "version": "{version}",
  "templates": [{{"id":"review","path":"templates/review.txt"}}]
}}"#
                ),
            )
            .expect("write manifest");
            install_package_manifest(&manifest_path, &install_root).expect("install package");
        };

        build_and_install("zeta", "2.0.0", "zeta template");
        build_and_install("alpha", "1.0.0", "alpha template");

        let report = list_installed_packages(&install_root).expect("list installed packages");
        assert_eq!(report.packages.len(), 2);
        assert_eq!(report.invalid_entries.len(), 0);
        assert_eq!(report.packages[0].name, "alpha");
        assert_eq!(report.packages[0].version, "1.0.0");
        assert_eq!(report.packages[1].name, "zeta");
        assert_eq!(report.packages[1].version, "2.0.0");
    }

    #[test]
    fn regression_list_installed_packages_records_invalid_manifest() {
        let temp = tempdir().expect("tempdir");
        let install_root = temp.path().join("installed");

        let source_root = temp.path().join("valid-source");
        std::fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
        std::fs::write(source_root.join("templates/review.txt"), "valid template")
            .expect("write template source");
        let valid_manifest = source_root.join("package.json");
        std::fs::write(
            &valid_manifest,
            r#"{
  "schema_version": 1,
  "name": "valid",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
        )
        .expect("write valid manifest");
        install_package_manifest(&valid_manifest, &install_root).expect("install valid package");

        let invalid_dir = install_root.join("broken/9.9.9");
        std::fs::create_dir_all(&invalid_dir).expect("create invalid dir");
        std::fs::write(
            invalid_dir.join("package.json"),
            r#"{
  "schema_version": 99,
  "name": "broken",
  "version": "9.9.9",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
        )
        .expect("write invalid manifest");

        let report = list_installed_packages(&install_root).expect("list installed packages");
        assert_eq!(report.packages.len(), 1);
        assert_eq!(report.invalid_entries.len(), 1);
        assert_eq!(report.packages[0].name, "valid");
        assert!(report.invalid_entries[0]
            .error
            .contains("unsupported package manifest schema"));
    }

    #[test]
    fn regression_list_installed_packages_missing_root_returns_empty_inventory() {
        let temp = tempdir().expect("tempdir");
        let report =
            list_installed_packages(&temp.path().join("missing-root")).expect("list succeeds");
        assert_eq!(report.packages.len(), 0);
        assert_eq!(report.invalid_entries.len(), 0);
    }

    #[test]
    fn unit_render_package_list_report_includes_inventory_and_invalid_sections() {
        let report = PackageListReport {
            list_root: Path::new("/tmp/packages").to_path_buf(),
            packages: vec![super::PackageListEntry {
                manifest_path: Path::new("/tmp/packages/starter/1.0.0/package.json").to_path_buf(),
                package_dir: Path::new("/tmp/packages/starter/1.0.0").to_path_buf(),
                name: "starter".to_string(),
                version: "1.0.0".to_string(),
                total_components: 2,
            }],
            invalid_entries: vec![super::PackageListInvalidEntry {
                package_dir: Path::new("/tmp/packages/broken/9.9.9").to_path_buf(),
                manifest_path: Path::new("/tmp/packages/broken/9.9.9/package.json").to_path_buf(),
                error: "unsupported package manifest schema".to_string(),
            }],
        };
        let rendered = render_package_list_report(&report);
        assert!(rendered.contains("package list:"));
        assert!(rendered.contains("packages=1"));
        assert!(rendered.contains("invalid=1"));
        assert!(rendered.contains("package: name=starter version=1.0.0"));
        assert!(rendered.contains("package invalid: path=/tmp/packages/broken/9.9.9"));
    }

    #[test]
    fn functional_remove_installed_package_deletes_bundle_and_empty_parent() {
        let temp = tempdir().expect("tempdir");
        let source_root = temp.path().join("source");
        std::fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
        std::fs::write(source_root.join("templates/review.txt"), "template")
            .expect("write template source");
        let manifest_path = source_root.join("package.json");
        std::fs::write(
            &manifest_path,
            r#"{
  "schema_version": 1,
  "name": "starter",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
        )
        .expect("write manifest");

        let install_root = temp.path().join("installed");
        install_package_manifest(&manifest_path, &install_root).expect("install package");
        let report = remove_installed_package("starter@1.0.0", &install_root)
            .expect("remove installed package");
        assert_eq!(report.status, PackageRemoveStatus::Removed);
        assert!(!install_root.join("starter/1.0.0").exists());
        assert!(!install_root.join("starter").exists());
    }

    #[test]
    fn functional_remove_installed_package_not_found_returns_not_found_status() {
        let temp = tempdir().expect("tempdir");
        let install_root = temp.path().join("installed");
        let report =
            remove_installed_package("starter@1.0.0", &install_root).expect("remove package");
        assert_eq!(report.status, PackageRemoveStatus::NotFound);
        assert_eq!(report.name, "starter");
        assert_eq!(report.version, "1.0.0");
    }

    #[test]
    fn regression_parse_package_coordinate_rejects_invalid_and_unsafe_inputs() {
        let format_error = parse_package_coordinate("starter").expect_err("format should fail");
        assert!(format_error
            .to_string()
            .contains("must follow <name>@<version>"));

        let version_error = parse_package_coordinate("starter@1.0").expect_err("version invalid");
        assert!(version_error.to_string().contains("must follow x.y.z"));

        let traversal_error =
            parse_package_coordinate("../evil@1.0.0").expect_err("unsafe name should fail");
        assert!(traversal_error
            .to_string()
            .contains("must not contain path separators"));
    }

    #[test]
    fn unit_parse_and_render_package_remove_report_are_deterministic() {
        let (name, version) =
            parse_package_coordinate("starter@2.3.4").expect("coordinate should parse");
        assert_eq!(name, "starter");
        assert_eq!(version, "2.3.4");

        let report = super::PackageRemoveReport {
            remove_root: Path::new("/tmp/packages").to_path_buf(),
            package_dir: Path::new("/tmp/packages/starter/2.3.4").to_path_buf(),
            name,
            version,
            status: PackageRemoveStatus::Removed,
        };
        let rendered = render_package_remove_report(&report);
        assert!(rendered.contains("package remove:"));
        assert!(rendered.contains("name=starter"));
        assert!(rendered.contains("version=2.3.4"));
        assert!(rendered.contains("status=removed"));
    }

    #[test]
    fn functional_rollback_installed_package_removes_non_target_versions() {
        let temp = tempdir().expect("tempdir");
        let install_root = temp.path().join("installed");
        let install_version = |version: &str, body: &str| {
            let source_root = temp.path().join(format!("source-{version}"));
            std::fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
            std::fs::write(source_root.join("templates/review.txt"), body)
                .expect("write template source");
            let manifest_path = source_root.join("package.json");
            std::fs::write(
                &manifest_path,
                format!(
                    r#"{{
  "schema_version": 1,
  "name": "starter",
  "version": "{version}",
  "templates": [{{"id":"review","path":"templates/review.txt"}}]
}}"#
                ),
            )
            .expect("write manifest");
            install_package_manifest(&manifest_path, &install_root).expect("install package");
        };

        install_version("1.0.0", "v1 template");
        install_version("2.0.0", "v2 template");

        let report = rollback_installed_package("starter@1.0.0", &install_root)
            .expect("rollback should succeed");
        assert_eq!(report.status, PackageRollbackStatus::RolledBack);
        assert_eq!(report.removed_versions, vec!["2.0.0".to_string()]);
        assert!(install_root.join("starter/1.0.0").exists());
        assert!(!install_root.join("starter/2.0.0").exists());
    }

    #[test]
    fn functional_rollback_installed_package_reports_already_at_target() {
        let temp = tempdir().expect("tempdir");
        let install_root = temp.path().join("installed");
        let source_root = temp.path().join("source");
        std::fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
        std::fs::write(source_root.join("templates/review.txt"), "v1 template")
            .expect("write template source");
        let manifest_path = source_root.join("package.json");
        std::fs::write(
            &manifest_path,
            r#"{
  "schema_version": 1,
  "name": "starter",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
        )
        .expect("write manifest");
        install_package_manifest(&manifest_path, &install_root).expect("install package");

        let report = rollback_installed_package("starter@1.0.0", &install_root)
            .expect("rollback should succeed");
        assert_eq!(report.status, PackageRollbackStatus::AlreadyAtTarget);
        assert_eq!(report.removed_versions.len(), 0);
    }

    #[test]
    fn regression_rollback_installed_package_rejects_missing_target() {
        let temp = tempdir().expect("tempdir");
        let install_root = temp.path().join("installed");
        let error = rollback_installed_package("starter@1.0.0", &install_root)
            .expect_err("missing target should fail");
        assert!(error.to_string().contains("is not installed"));
    }

    #[test]
    fn unit_render_package_rollback_report_includes_removed_versions() {
        let report = super::PackageRollbackReport {
            rollback_root: Path::new("/tmp/packages").to_path_buf(),
            package_name: "starter".to_string(),
            target_version: "1.0.0".to_string(),
            removed_versions: vec!["2.0.0".to_string(), "3.0.0".to_string()],
            status: PackageRollbackStatus::RolledBack,
        };
        let rendered = render_package_rollback_report(&report);
        assert!(rendered.contains("package rollback:"));
        assert!(rendered.contains("target_version=1.0.0"));
        assert!(rendered.contains("removed_versions=2"));
        assert!(rendered.contains("status=rolled_back"));
        assert!(rendered.contains("rollback removed: version=2.0.0"));
        assert!(rendered.contains("rollback removed: version=3.0.0"));
    }
}
