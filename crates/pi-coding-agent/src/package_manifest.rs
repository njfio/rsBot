use std::{
    path::{Component, Path, PathBuf},
    str::FromStr,
};

use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{Signature, VerifyingKey};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{Cli, TrustedKey};

const PACKAGE_MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct PackageManifestSummary {
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
pub(crate) struct PackageInstallReport {
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
pub(crate) struct PackageListEntry {
    pub manifest_path: PathBuf,
    pub package_dir: PathBuf,
    pub name: String,
    pub version: String,
    pub total_components: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PackageListInvalidEntry {
    pub package_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PackageListReport {
    pub list_root: PathBuf,
    pub packages: Vec<PackageListEntry>,
    pub invalid_entries: Vec<PackageListInvalidEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PackageRemoveReport {
    pub remove_root: PathBuf,
    pub package_dir: PathBuf,
    pub name: String,
    pub version: String,
    pub status: PackageRemoveStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PackageRemoveStatus {
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
pub(crate) struct PackageRollbackReport {
    pub rollback_root: PathBuf,
    pub package_name: String,
    pub target_version: String,
    pub removed_versions: Vec<String>,
    pub status: PackageRollbackStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PackageRollbackStatus {
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
pub(crate) enum FileUpsertOutcome {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PackageManifest {
    schema_version: u32,
    name: String,
    version: String,
    #[serde(default)]
    signing_key: Option<String>,
    #[serde(default)]
    signature_file: Option<String>,
    #[serde(default)]
    templates: Vec<PackageComponent>,
    #[serde(default)]
    skills: Vec<PackageComponent>,
    #[serde(default)]
    extensions: Vec<PackageComponent>,
    #[serde(default)]
    themes: Vec<PackageComponent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PackageComponent {
    id: String,
    path: String,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    sha256: Option<String>,
}

pub(crate) fn execute_package_validate_command(cli: &Cli) -> Result<()> {
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

pub(crate) fn execute_package_show_command(cli: &Cli) -> Result<()> {
    let Some(path) = cli.package_show.as_ref() else {
        return Ok(());
    };
    let (manifest, summary) = load_and_validate_manifest(path)?;
    println!("{}", render_package_manifest_report(&summary, &manifest));
    Ok(())
}

pub(crate) fn execute_package_install_command(cli: &Cli) -> Result<()> {
    let Some(path) = cli.package_install.as_ref() else {
        return Ok(());
    };
    let trusted_roots = crate::resolve_skill_trust_roots(cli)?;
    let report = install_package_manifest_with_policy(
        path,
        &cli.package_install_root,
        cli.require_signed_packages,
        &trusted_roots,
    )?;
    println!("{}", render_package_install_report(&report));
    Ok(())
}

pub(crate) fn execute_package_list_command(cli: &Cli) -> Result<()> {
    if !cli.package_list {
        return Ok(());
    }
    let report = list_installed_packages(&cli.package_list_root)?;
    println!("{}", render_package_list_report(&report));
    Ok(())
}

pub(crate) fn execute_package_remove_command(cli: &Cli) -> Result<()> {
    let Some(coordinate) = cli.package_remove.as_deref() else {
        return Ok(());
    };
    let report = remove_installed_package(coordinate, &cli.package_remove_root)?;
    println!("{}", render_package_remove_report(&report));
    Ok(())
}

pub(crate) fn execute_package_rollback_command(cli: &Cli) -> Result<()> {
    let Some(coordinate) = cli.package_rollback.as_deref() else {
        return Ok(());
    };
    let report = rollback_installed_package(coordinate, &cli.package_rollback_root)?;
    println!("{}", render_package_rollback_report(&report));
    Ok(())
}

pub(crate) fn validate_package_manifest(path: &Path) -> Result<PackageManifestSummary> {
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
    format!(
        "package install: manifest={} root={} package_dir={} name={} version={} manifest_status={} installed={} updated={} skipped={} total_components={}",
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
        let relative_path = PathBuf::from_str(component.path.trim())
            .map_err(|_| anyhow!("failed to parse {} path '{}'", kind, component.path.trim()))?;
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

fn resolve_component_source_path(
    kind: &str,
    id: &str,
    relative_path: &Path,
    canonical_manifest_dir: &Path,
) -> Result<PathBuf> {
    let joined = canonical_manifest_dir.join(relative_path);
    let canonical_source = std::fs::canonicalize(&joined).with_context(|| {
        format!(
            "package manifest {} entry '{}' source '{}' does not exist",
            kind,
            id,
            relative_path.display()
        )
    })?;
    if !canonical_source.starts_with(canonical_manifest_dir) {
        bail!(
            "package manifest {} entry '{}' source '{}' resolves outside package manifest directory",
            kind,
            id,
            relative_path.display()
        );
    }
    let metadata = std::fs::metadata(&canonical_source).with_context(|| {
        format!(
            "failed to read metadata for package manifest {} entry '{}' source '{}'",
            kind,
            id,
            canonical_source.display()
        )
    })?;
    if !metadata.is_file() {
        bail!(
            "package manifest {} entry '{}' source '{}' must be a file",
            kind,
            id,
            relative_path.display()
        );
    }
    Ok(canonical_source)
}

fn resolve_component_source_bytes(
    kind: &str,
    id: &str,
    component: &PackageComponent,
    relative_path: &Path,
    canonical_manifest_dir: &Path,
) -> Result<Vec<u8>> {
    let bytes = if let Some(raw_url) = component.url.as_deref() {
        fetch_remote_component_source(kind, id, raw_url)?
    } else {
        load_local_component_source(kind, id, relative_path, canonical_manifest_dir)?
    };
    if let Some(raw_checksum) = component.sha256.as_deref() {
        verify_component_checksum(kind, id, raw_checksum, &bytes)?;
    }
    Ok(bytes)
}

fn load_local_component_source(
    kind: &str,
    id: &str,
    relative_path: &Path,
    canonical_manifest_dir: &Path,
) -> Result<Vec<u8>> {
    let source_path =
        resolve_component_source_path(kind, id, relative_path, canonical_manifest_dir)?;
    std::fs::read(&source_path).with_context(|| {
        format!(
            "failed to read source file {} for package manifest {} entry '{}'",
            source_path.display(),
            kind,
            id
        )
    })
}

fn fetch_remote_component_source(kind: &str, id: &str, raw_url: &str) -> Result<Vec<u8>> {
    let source_url = parse_component_source_url(kind, id, raw_url)?;
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| {
            handle.block_on(fetch_remote_component_source_async(kind, id, &source_url))
        })
    } else {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("failed to create runtime for package remote source download")?;
        runtime.block_on(fetch_remote_component_source_async(kind, id, &source_url))
    }
}

async fn fetch_remote_component_source_async(
    kind: &str,
    id: &str,
    source_url: &Url,
) -> Result<Vec<u8>> {
    let response = reqwest::Client::new()
        .get(source_url.clone())
        .send()
        .await
        .with_context(|| {
            format!(
                "failed to fetch package manifest {} entry '{}' url '{}'",
                kind, id, source_url
            )
        })?;
    if !response.status().is_success() {
        bail!(
            "failed to fetch package manifest {} entry '{}' url '{}' with status {}",
            kind,
            id,
            source_url,
            response.status()
        );
    }
    response
        .bytes()
        .await
        .with_context(|| {
            format!(
                "failed to read response body for package manifest {} entry '{}' url '{}'",
                kind, id, source_url
            )
        })
        .map(|bytes| bytes.to_vec())
}

fn parse_component_source_url(kind: &str, id: &str, raw_url: &str) -> Result<Url> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        bail!(
            "package manifest {} entry '{}' url must be non-empty",
            kind,
            id
        );
    }
    let source_url = Url::parse(trimmed).with_context(|| {
        format!(
            "package manifest {} entry '{}' url '{}' is invalid",
            kind, id, trimmed
        )
    })?;
    if !matches!(source_url.scheme(), "http" | "https") {
        bail!(
            "package manifest {} entry '{}' url '{}' must use http or https",
            kind,
            id,
            trimmed
        );
    }
    Ok(source_url)
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

fn parse_sha256_checksum(raw_checksum: &str) -> Result<String> {
    let trimmed = raw_checksum.trim();
    if trimmed.is_empty() {
        bail!("sha256 checksum must be non-empty");
    }
    let hex = trimmed.strip_prefix("sha256:").unwrap_or(trimmed);
    if hex.len() != 64 || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        bail!("sha256 checksum must use format sha256:<64 hex characters>");
    }
    Ok(hex.to_ascii_lowercase())
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

fn validate_component_set(kind: &str, components: &[PackageComponent]) -> Result<()> {
    let mut seen_ids = std::collections::BTreeSet::new();
    for component in components {
        let id = component.id.trim();
        if id.is_empty() {
            bail!("package manifest {} entry id must be non-empty", kind);
        }
        if !seen_ids.insert(id.to_string()) {
            bail!("duplicate {} id '{}'", kind, id);
        }
        validate_relative_component_path(kind, id, component.path.trim())?;
        if let Some(raw_url) = component.url.as_deref() {
            parse_component_source_url(kind, id, raw_url)?;
        }
        if let Some(raw_checksum) = component.sha256.as_deref() {
            parse_sha256_checksum(raw_checksum).with_context(|| {
                format!(
                    "package manifest {} entry '{}' has invalid checksum '{}'",
                    kind,
                    id,
                    raw_checksum.trim()
                )
            })?;
        }
    }
    Ok(())
}

fn validate_relative_component_path(kind: &str, id: &str, raw_path: &str) -> Result<()> {
    if raw_path.is_empty() {
        bail!(
            "package manifest {} entry '{}' path must be non-empty",
            kind,
            id
        );
    }
    let path = PathBuf::from_str(raw_path)
        .map_err(|_| anyhow!("failed to parse {} path '{}'", kind, raw_path))?;
    if path.is_absolute() {
        bail!(
            "package manifest {} entry '{}' path '{}' must be relative",
            kind,
            id,
            raw_path
        );
    }
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        bail!(
            "package manifest {} entry '{}' path '{}' must not contain parent traversals",
            kind,
            id,
            raw_path
        );
    }
    Ok(())
}

fn is_semver_like(raw: &str) -> bool {
    let mut parts = raw.split('.');
    let major = parts.next();
    let minor = parts.next();
    let patch = parts.next();
    if parts.next().is_some() {
        return false;
    }
    [major, minor, patch].into_iter().all(|part| {
        part.map(|value| !value.is_empty() && value.chars().all(|ch| ch.is_ascii_digit()))
            .unwrap_or(false)
    })
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
        install_package_manifest, install_package_manifest_with_policy, list_installed_packages,
        load_and_validate_manifest, parse_package_coordinate, remove_installed_package,
        render_package_install_report, render_package_list_report, render_package_manifest_report,
        render_package_remove_report, render_package_rollback_report, rollback_installed_package,
        validate_package_manifest, FileUpsertOutcome, PackageListReport, PackageRemoveStatus,
        PackageRollbackStatus,
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
