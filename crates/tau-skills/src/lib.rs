//! Skill package management, trust, and verification runtime for Tau.
//!
//! Handles skill manifest parsing, install/update/remove, lockfile sync, trust
//! records, and signature verification workflows.

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{Signature, VerifyingKey};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod commands;
mod package_manifest;
pub mod trust_roots;

pub use commands::*;
pub use package_manifest::*;
pub use trust_roots::*;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `Skill` used across Tau components.
pub struct Skill {
    pub name: String,
    pub content: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
/// Public struct `SkillInstallReport` used across Tau components.
pub struct SkillInstallReport {
    pub installed: usize,
    pub updated: usize,
    pub skipped: usize,
}

const SKILLS_LOCK_SCHEMA_VERSION: u32 = 1;
const SKILLS_LOCK_FILE_NAME: &str = "skills.lock.json";
const SKILLS_CACHE_DIR_NAME: &str = ".cache";
const SKILLS_CACHE_ARTIFACTS_DIR: &str = "artifacts";
const SKILLS_CACHE_MANIFESTS_DIR: &str = "manifests";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
/// Public struct `SkillsDownloadOptions` used across Tau components.
pub struct SkillsDownloadOptions {
    pub cache_dir: Option<PathBuf>,
    pub offline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `RemoteSkillSource` used across Tau components.
pub struct RemoteSkillSource {
    pub url: String,
    pub sha256: Option<String>,
    pub signing_key_id: Option<String>,
    pub signature: Option<String>,
    pub signer_public_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
/// Enumerates supported `SkillLockSource` values.
pub enum SkillLockSource {
    Unknown,
    Local {
        path: String,
    },
    Remote {
        url: String,
        expected_sha256: Option<String>,
        signing_key_id: Option<String>,
        signature: Option<String>,
        signer_public_key: Option<String>,
        signature_sha256: Option<String>,
    },
    Registry {
        registry_url: String,
        name: String,
        url: String,
        expected_sha256: Option<String>,
        signing_key_id: Option<String>,
        signature: Option<String>,
        signer_public_key: Option<String>,
        signature_sha256: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Public struct `SkillLockEntry` used across Tau components.
pub struct SkillLockEntry {
    pub name: String,
    pub file: String,
    pub sha256: String,
    pub source: SkillLockSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Public struct `SkillsLockfile` used across Tau components.
pub struct SkillsLockfile {
    pub schema_version: u32,
    pub entries: Vec<SkillLockEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `SkillLockHint` used across Tau components.
pub struct SkillLockHint {
    pub file: String,
    pub source: SkillLockSource,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
/// Public struct `SkillsSyncReport` used across Tau components.
pub struct SkillsSyncReport {
    pub expected_entries: usize,
    pub actual_entries: usize,
    pub missing: Vec<String>,
    pub extra: Vec<String>,
    pub changed: Vec<String>,
    pub metadata_mismatch: Vec<String>,
}

impl SkillsSyncReport {
    pub fn in_sync(&self) -> bool {
        self.missing.is_empty()
            && self.extra.is_empty()
            && self.changed.is_empty()
            && self.metadata_mismatch.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `TrustedKey` used across Tau components.
pub struct TrustedKey {
    pub id: String,
    pub public_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
/// Public struct `RegistrySkillEntry` used across Tau components.
pub struct RegistrySkillEntry {
    pub name: String,
    pub url: String,
    pub sha256: Option<String>,
    pub signing_key: Option<String>,
    pub signature: Option<String>,
    #[serde(default)]
    pub revoked: bool,
    pub expires_unix: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
/// Public struct `RegistryKeyEntry` used across Tau components.
pub struct RegistryKeyEntry {
    pub id: String,
    pub public_key: String,
    pub signed_by: Option<String>,
    pub signature: Option<String>,
    #[serde(default)]
    pub revoked: bool,
    pub expires_unix: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
/// Public struct `SkillRegistryManifest` used across Tau components.
pub struct SkillRegistryManifest {
    pub version: u32,
    #[serde(default)]
    pub keys: Vec<RegistryKeyEntry>,
    pub skills: Vec<RegistrySkillEntry>,
}

pub fn load_catalog(dir: &Path) -> Result<Vec<Skill>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    if !dir.is_dir() {
        bail!("skills path '{}' is not a directory", dir.display());
    }

    let mut skills = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry.with_context(|| format!("failed to read entry in {}", dir.display()))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }

        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read skill file {}", path.display()))?;
        skills.push(Skill {
            name: stem.to_string(),
            content,
            path,
        });
    }

    skills.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(skills)
}

pub fn install_skills(sources: &[PathBuf], destination_dir: &Path) -> Result<SkillInstallReport> {
    if sources.is_empty() {
        return Ok(SkillInstallReport::default());
    }

    fs::create_dir_all(destination_dir)
        .with_context(|| format!("failed to create {}", destination_dir.display()))?;

    let mut report = SkillInstallReport::default();
    for source in sources {
        if source.extension().and_then(|ext| ext.to_str()) != Some("md") {
            bail!("skill source '{}' must be a .md file", source.display());
        }

        let file_name = source
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("invalid skill source '{}'", source.display()))?;
        let destination = destination_dir.join(file_name);

        let content = fs::read_to_string(source)
            .with_context(|| format!("failed to read skill source {}", source.display()))?;
        upsert_skill_file(&destination, &content, &mut report)?;
    }

    Ok(report)
}

pub fn resolve_remote_skill_sources(
    urls: &[String],
    sha256_values: &[String],
) -> Result<Vec<RemoteSkillSource>> {
    if sha256_values.is_empty() {
        return Ok(urls
            .iter()
            .map(|url| RemoteSkillSource {
                url: url.clone(),
                sha256: None,
                signing_key_id: None,
                signature: None,
                signer_public_key: None,
            })
            .collect());
    }

    if urls.len() != sha256_values.len() {
        bail!(
            "--install-skill-url count ({}) must match --install-skill-sha256 count ({})",
            urls.len(),
            sha256_values.len()
        );
    }

    Ok(urls
        .iter()
        .zip(sha256_values.iter())
        .map(|(url, sha256)| RemoteSkillSource {
            url: url.clone(),
            sha256: Some(sha256.clone()),
            signing_key_id: None,
            signature: None,
            signer_public_key: None,
        })
        .collect())
}

pub async fn install_remote_skills_with_cache(
    sources: &[RemoteSkillSource],
    destination_dir: &Path,
    options: &SkillsDownloadOptions,
) -> Result<SkillInstallReport> {
    if sources.is_empty() {
        return Ok(SkillInstallReport::default());
    }

    fs::create_dir_all(destination_dir)
        .with_context(|| format!("failed to create {}", destination_dir.display()))?;

    let client = reqwest::Client::new();
    let mut report = SkillInstallReport::default();

    for (index, source) in sources.iter().enumerate() {
        let url = Url::parse(&source.url)
            .with_context(|| format!("invalid skill URL '{}'", source.url))?;
        if !matches!(url.scheme(), "http" | "https") {
            bail!("unsupported skill URL scheme '{}'", url.scheme());
        }

        let cache_path = options
            .cache_dir
            .as_deref()
            .map(|cache_dir| remote_skill_cache_path(cache_dir, &source.url));
        let bytes = fetch_remote_skill_bytes(
            &client,
            &url,
            source,
            cache_path.as_deref(),
            options.offline,
        )
        .await?;

        let file_name = remote_skill_file_name(&url, index);
        let destination = destination_dir.join(file_name);
        let content = String::from_utf8(bytes)
            .with_context(|| format!("skill content from '{}' is not UTF-8", source.url))?;

        upsert_skill_file(&destination, &content, &mut report)?;
    }

    Ok(report)
}

async fn fetch_remote_skill_bytes(
    client: &reqwest::Client,
    url: &Url,
    source: &RemoteSkillSource,
    cache_path: Option<&Path>,
    offline: bool,
) -> Result<Vec<u8>> {
    validate_remote_skill_source_metadata(source)?;
    if offline {
        let cache_path = cache_path
            .ok_or_else(|| anyhow!("offline mode requires a configured skills cache directory"))?;
        let bytes = read_cached_payload(cache_path)
            .with_context(|| format!("offline cache miss for skill URL '{}'", source.url))?;
        validate_remote_skill_payload(source, &bytes).with_context(|| {
            format!(
                "cached skill payload validation failed for '{}'",
                source.url
            )
        })?;
        return Ok(bytes);
    }

    if let Some(cache_path) = cache_path {
        if should_reuse_cached_remote_payload(source) {
            if let Ok(bytes) = read_cached_payload(cache_path) {
                if validate_remote_skill_payload(source, &bytes).is_ok() {
                    return Ok(bytes);
                }
            }
        }
    }

    let response = client
        .get(url.clone())
        .send()
        .await
        .with_context(|| format!("failed to fetch skill URL '{}'", source.url))?;
    if !response.status().is_success() {
        bail!(
            "failed to fetch skill URL '{}' with status {}",
            source.url,
            response.status()
        );
    }
    let bytes = response
        .bytes()
        .await
        .with_context(|| format!("failed to read skill response '{}'", source.url))?
        .to_vec();

    validate_remote_skill_payload(source, &bytes)?;
    if let Some(cache_path) = cache_path {
        write_cached_payload(cache_path, &bytes)
            .with_context(|| format!("failed to cache skill URL '{}'", source.url))?;
    }
    Ok(bytes)
}

fn validate_remote_skill_source_metadata(source: &RemoteSkillSource) -> Result<()> {
    if source.signature.is_some() ^ source.signer_public_key.is_some() {
        bail!(
            "incomplete signature metadata for '{}': both signature and signer public key are required",
            source.url
        );
    }
    Ok(())
}

fn validate_remote_skill_payload(source: &RemoteSkillSource, bytes: &[u8]) -> Result<()> {
    if let Some(expected_sha256) = &source.sha256 {
        let actual_sha256 = sha256_hex(bytes);
        let expected_sha256 = normalize_sha256(expected_sha256);
        if actual_sha256 != expected_sha256 {
            bail!(
                "sha256 mismatch for '{}': expected {}, got {}",
                source.url,
                expected_sha256,
                actual_sha256
            );
        }
    }

    if let (Some(signature), Some(signer_public_key)) =
        (&source.signature, &source.signer_public_key)
    {
        verify_ed25519_signature(bytes, signature, signer_public_key)
            .with_context(|| format!("signature verification failed for '{}'", source.url))?;
    }
    Ok(())
}

fn should_reuse_cached_remote_payload(source: &RemoteSkillSource) -> bool {
    source.sha256.is_some() || source.signature.is_some()
}

pub fn default_skills_lock_path(skills_dir: &Path) -> PathBuf {
    skills_dir.join(SKILLS_LOCK_FILE_NAME)
}

pub fn default_skills_cache_dir(skills_dir: &Path) -> PathBuf {
    skills_dir.join(SKILLS_CACHE_DIR_NAME)
}

pub fn build_local_skill_lock_hints(sources: &[PathBuf]) -> Result<Vec<SkillLockHint>> {
    let mut hints = Vec::new();
    for source in sources {
        if source.extension().and_then(|ext| ext.to_str()) != Some("md") {
            bail!("skill source '{}' must be a .md file", source.display());
        }
        let file = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow!("invalid skill source '{}'", source.display()))?
            .to_string();
        hints.push(SkillLockHint {
            file,
            source: SkillLockSource::Local {
                path: source.display().to_string(),
            },
        });
    }
    Ok(hints)
}

pub fn build_remote_skill_lock_hints(sources: &[RemoteSkillSource]) -> Result<Vec<SkillLockHint>> {
    let mut hints = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        let file = remote_skill_file_name_for_source(&source.url, index)?;
        hints.push(SkillLockHint {
            file,
            source: SkillLockSource::Remote {
                url: source.url.clone(),
                expected_sha256: source.sha256.clone(),
                signing_key_id: source.signing_key_id.clone(),
                signature: source.signature.clone(),
                signer_public_key: source.signer_public_key.clone(),
                signature_sha256: source.signature.as_deref().map(signature_digest_sha256_hex),
            },
        });
    }
    Ok(hints)
}

pub fn build_registry_skill_lock_hints(
    registry_url: &str,
    names: &[String],
    sources: &[RemoteSkillSource],
) -> Result<Vec<SkillLockHint>> {
    if names.len() != sources.len() {
        bail!(
            "registry lock hint metadata mismatch: names={}, sources={}",
            names.len(),
            sources.len()
        );
    }
    let mut hints = Vec::new();
    for (index, (name, source)) in names.iter().zip(sources.iter()).enumerate() {
        let file = remote_skill_file_name_for_source(&source.url, index)?;
        hints.push(SkillLockHint {
            file,
            source: SkillLockSource::Registry {
                registry_url: registry_url.to_string(),
                name: name.clone(),
                url: source.url.clone(),
                expected_sha256: source.sha256.clone(),
                signing_key_id: source.signing_key_id.clone(),
                signature: source.signature.clone(),
                signer_public_key: source.signer_public_key.clone(),
                signature_sha256: source.signature.as_deref().map(signature_digest_sha256_hex),
            },
        });
    }
    Ok(hints)
}

pub fn remote_skill_file_name_for_source(url: &str, index: usize) -> Result<String> {
    let url = Url::parse(url).with_context(|| format!("invalid skill URL '{}'", url))?;
    Ok(remote_skill_file_name(&url, index))
}

pub fn load_skills_lockfile(path: &Path) -> Result<SkillsLockfile> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read skills lockfile {}", path.display()))?;
    let lockfile: SkillsLockfile = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse skills lockfile {}", path.display()))?;
    validate_skills_lockfile(&lockfile, path)?;
    Ok(lockfile)
}

pub fn write_skills_lockfile(
    skills_dir: &Path,
    lock_path: &Path,
    hints: &[SkillLockHint],
) -> Result<SkillsLockfile> {
    if !skills_dir.exists() {
        fs::create_dir_all(skills_dir)
            .with_context(|| format!("failed to create {}", skills_dir.display()))?;
    }

    let mut hint_by_file = HashMap::new();
    for hint in hints {
        hint_by_file.insert(hint.file.clone(), hint.source.clone());
    }

    let mut existing_source_by_file = HashMap::new();
    if lock_path.exists() {
        let existing = load_skills_lockfile(lock_path)?;
        for entry in existing.entries {
            existing_source_by_file.insert(entry.file, entry.source);
        }
    }

    let catalog = load_catalog(skills_dir)?;
    let mut entries = Vec::new();
    for skill in catalog {
        let file = skill
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow!("invalid installed skill path '{}'", skill.path.display()))?
            .to_string();
        let source = hint_by_file
            .get(&file)
            .cloned()
            .or_else(|| existing_source_by_file.get(&file).cloned())
            .unwrap_or(SkillLockSource::Unknown);
        entries.push(SkillLockEntry {
            name: skill.name,
            file,
            sha256: sha256_hex(skill.content.as_bytes()),
            source,
        });
    }
    entries.sort_by(|left, right| left.file.cmp(&right.file));

    let lockfile = SkillsLockfile {
        schema_version: SKILLS_LOCK_SCHEMA_VERSION,
        entries,
    };
    validate_skills_lockfile(&lockfile, lock_path)?;

    if let Some(parent) = lock_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create lockfile directory {}", parent.display())
            })?;
        }
    }
    let mut encoded = serde_json::to_vec_pretty(&lockfile).context("failed to encode lockfile")?;
    encoded.push(b'\n');
    fs::write(lock_path, encoded)
        .with_context(|| format!("failed to write skills lockfile {}", lock_path.display()))?;

    Ok(lockfile)
}

pub fn sync_skills_with_lockfile(skills_dir: &Path, lock_path: &Path) -> Result<SkillsSyncReport> {
    let lockfile = load_skills_lockfile(lock_path)?;

    let mut expected = HashMap::new();
    for entry in &lockfile.entries {
        expected.insert(entry.file.clone(), entry.sha256.clone());
    }

    let mut actual = HashMap::new();
    for skill in load_catalog(skills_dir)? {
        let file = skill
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow!("invalid installed skill path '{}'", skill.path.display()))?
            .to_string();
        actual.insert(file, sha256_hex(skill.content.as_bytes()));
    }

    let mut report = SkillsSyncReport {
        expected_entries: expected.len(),
        actual_entries: actual.len(),
        ..SkillsSyncReport::default()
    };
    for entry in &lockfile.entries {
        if let Some(reason) = lock_entry_metadata_mismatch(entry) {
            report
                .metadata_mismatch
                .push(format!("{}: {}", entry.file, reason));
        }
    }

    for (file, expected_sha) in &expected {
        match actual.get(file) {
            None => report.missing.push(file.clone()),
            Some(actual_sha) if actual_sha != expected_sha => report.changed.push(file.clone()),
            Some(_) => {}
        }
    }
    for file in actual.keys() {
        if !expected.contains_key(file) {
            report.extra.push(file.clone());
        }
    }

    report.missing.sort();
    report.extra.sort();
    report.changed.sort();
    report.metadata_mismatch.sort();

    Ok(report)
}

pub async fn fetch_registry_manifest_with_cache(
    registry_url: &str,
    expected_sha256: Option<&str>,
    options: &SkillsDownloadOptions,
) -> Result<SkillRegistryManifest> {
    let url = Url::parse(registry_url)
        .with_context(|| format!("invalid registry URL '{}'", registry_url))?;
    if !matches!(url.scheme(), "http" | "https") {
        bail!("unsupported registry URL scheme '{}'", url.scheme());
    }

    let cache_path = options
        .cache_dir
        .as_deref()
        .map(|cache_dir| registry_manifest_cache_path(cache_dir, registry_url));
    if options.offline {
        let cache_path = cache_path
            .ok_or_else(|| anyhow!("offline mode requires a configured skills cache directory"))?;
        let bytes = read_cached_payload(&cache_path)
            .with_context(|| format!("offline cache miss for registry '{}'", registry_url))?;
        validate_registry_manifest_checksum(registry_url, expected_sha256, &bytes)?;
        return parse_registry_manifest(registry_url, &bytes);
    }

    if let Some(cache_path) = cache_path.as_deref() {
        if expected_sha256.is_some() {
            if let Ok(bytes) = read_cached_payload(cache_path) {
                if validate_registry_manifest_checksum(registry_url, expected_sha256, &bytes)
                    .is_ok()
                {
                    if let Ok(manifest) = parse_registry_manifest(registry_url, &bytes) {
                        return Ok(manifest);
                    }
                }
            }
        }
    }

    let response = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .with_context(|| format!("failed to fetch registry '{}'", registry_url))?;
    if !response.status().is_success() {
        bail!(
            "failed to fetch registry '{}' with status {}",
            registry_url,
            response.status()
        );
    }
    let bytes = response
        .bytes()
        .await
        .with_context(|| format!("failed to read registry response '{}'", registry_url))?
        .to_vec();

    validate_registry_manifest_checksum(registry_url, expected_sha256, &bytes)?;
    let manifest = parse_registry_manifest(registry_url, &bytes)?;
    if let Some(cache_path) = cache_path.as_deref() {
        write_cached_payload(cache_path, &bytes)
            .with_context(|| format!("failed to cache registry '{}'", registry_url))?;
    }
    Ok(manifest)
}

fn parse_registry_manifest(registry_url: &str, bytes: &[u8]) -> Result<SkillRegistryManifest> {
    let manifest = serde_json::from_slice::<SkillRegistryManifest>(bytes)
        .with_context(|| format!("failed to parse registry '{}'", registry_url))?;
    if manifest.version == 0 {
        bail!("registry '{}' has invalid version 0", registry_url);
    }
    Ok(manifest)
}

fn validate_registry_manifest_checksum(
    registry_url: &str,
    expected_sha256: Option<&str>,
    bytes: &[u8],
) -> Result<()> {
    if let Some(expected_sha256) = expected_sha256 {
        let actual_sha256 = sha256_hex(bytes);
        let expected_sha256 = normalize_sha256(expected_sha256);
        if actual_sha256 != expected_sha256 {
            bail!(
                "registry sha256 mismatch for '{}': expected {}, got {}",
                registry_url,
                expected_sha256,
                actual_sha256
            );
        }
    }
    Ok(())
}

pub fn resolve_registry_skill_sources(
    manifest: &SkillRegistryManifest,
    selected_names: &[String],
    trust_roots: &[TrustedKey],
    require_signed: bool,
) -> Result<Vec<RemoteSkillSource>> {
    let now_unix = current_unix_timestamp();
    let trusted_keys = build_trusted_key_map(manifest, trust_roots)?;
    let mut resolved = Vec::new();
    for name in selected_names {
        let entry = manifest
            .skills
            .iter()
            .find(|entry| entry.name == *name)
            .ok_or_else(|| anyhow!("registry does not contain skill '{}'", name))?;

        if entry.revoked {
            bail!("registry skill '{}' is revoked", name);
        }
        if is_expired(entry.expires_unix, now_unix) {
            bail!("registry skill '{}' is expired", name);
        }

        let has_signature = entry.signature.is_some() || entry.signing_key.is_some();
        if require_signed && !has_signature {
            bail!(
                "registry skill '{}' is unsigned but signatures are required",
                name
            );
        }

        if entry.signature.is_some() ^ entry.signing_key.is_some() {
            bail!(
                "registry skill '{}' has incomplete signing metadata (both signing_key and signature are required)",
                name
            );
        }

        let (signature, signer_public_key) =
            if let (Some(signature), Some(signing_key)) = (&entry.signature, &entry.signing_key) {
                let signer_public_key = trusted_keys.get(signing_key).ok_or_else(|| {
                    let maybe_signing_key = manifest.keys.iter().find(|key| key.id == *signing_key);
                    if let Some(signing_key_entry) = maybe_signing_key {
                        if signing_key_entry.revoked {
                            anyhow!(
                                "registry skill '{}' uses revoked signing key '{}'",
                                name,
                                signing_key
                            )
                        } else if is_expired(signing_key_entry.expires_unix, now_unix) {
                            anyhow!(
                                "registry skill '{}' uses expired signing key '{}'",
                                name,
                                signing_key
                            )
                        } else {
                            anyhow!(
                                "registry skill '{}' uses untrusted signing key '{}'",
                                name,
                                signing_key
                            )
                        }
                    } else {
                        anyhow!(
                            "registry skill '{}' uses untrusted signing key '{}'",
                            name,
                            signing_key
                        )
                    }
                })?;
                (Some(signature.clone()), Some(signer_public_key.clone()))
            } else {
                (None, None)
            };

        resolved.push(RemoteSkillSource {
            url: entry.url.clone(),
            sha256: entry.sha256.clone(),
            signing_key_id: entry.signing_key.clone(),
            signature,
            signer_public_key,
        });
    }

    Ok(resolved)
}

pub fn resolve_selected_skills(catalog: &[Skill], selected: &[String]) -> Result<Vec<Skill>> {
    let mut resolved = Vec::new();
    for name in selected {
        let skill = catalog
            .iter()
            .find(|skill| skill.name == *name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown skill '{}'", name))?;
        resolved.push(skill);
    }

    Ok(resolved)
}

pub fn augment_system_prompt(base: &str, skills: &[Skill]) -> String {
    let mut prompt = base.trim_end().to_string();
    for skill in skills {
        if !prompt.is_empty() {
            prompt.push_str("\n\n");
        }

        prompt.push_str("# Skill: ");
        prompt.push_str(&skill.name);
        prompt.push('\n');
        prompt.push_str(skill.content.trim());
    }

    prompt
}

fn validate_skills_lockfile(lockfile: &SkillsLockfile, path: &Path) -> Result<()> {
    if lockfile.schema_version != SKILLS_LOCK_SCHEMA_VERSION {
        bail!(
            "unsupported skills lockfile schema_version {} in {} (expected {})",
            lockfile.schema_version,
            path.display(),
            SKILLS_LOCK_SCHEMA_VERSION
        );
    }

    let mut seen_files = HashSet::new();
    for entry in &lockfile.entries {
        if entry.file.trim().is_empty() {
            bail!(
                "skills lockfile {} contains an entry with empty file name",
                path.display()
            );
        }
        if !entry.file.ends_with(".md") {
            bail!(
                "skills lockfile {} contains non-markdown entry '{}'",
                path.display(),
                entry.file
            );
        }
        if !seen_files.insert(entry.file.clone()) {
            bail!(
                "skills lockfile {} contains duplicate entry '{}'",
                path.display(),
                entry.file
            );
        }
        if entry.sha256.trim().is_empty() {
            bail!(
                "skills lockfile {} contains empty sha256 for '{}'",
                path.display(),
                entry.file
            );
        }
    }

    Ok(())
}

fn lock_entry_metadata_mismatch(entry: &SkillLockEntry) -> Option<String> {
    match &entry.source {
        SkillLockSource::Remote {
            expected_sha256,
            signature,
            signature_sha256,
            ..
        }
        | SkillLockSource::Registry {
            expected_sha256,
            signature,
            signature_sha256,
            ..
        } => {
            if let Some(expected_sha256) = expected_sha256 {
                let normalized_expected = normalize_sha256(expected_sha256);
                if normalized_expected != entry.sha256 {
                    return Some(format!(
                        "expected_sha256 {} does not match lockfile sha256 {}",
                        normalized_expected, entry.sha256
                    ));
                }
            }

            match (signature.as_deref(), signature_sha256.as_deref()) {
                (Some(signature), Some(signature_sha256)) => {
                    let expected_digest = normalize_sha256(signature_sha256);
                    let actual_digest = signature_digest_sha256_hex(signature);
                    if expected_digest != actual_digest {
                        return Some(format!(
                            "signature_sha256 {} does not match computed digest {}",
                            expected_digest, actual_digest
                        ));
                    }
                }
                (Some(_), None) => {
                    return Some("signature_sha256 missing for signed source".to_string())
                }
                (None, Some(_)) => {
                    return Some("signature_sha256 is present without signature".to_string())
                }
                (None, None) => {}
            }
        }
        SkillLockSource::Unknown | SkillLockSource::Local { .. } => {}
    }
    None
}

fn build_trusted_key_map(
    manifest: &SkillRegistryManifest,
    trust_roots: &[TrustedKey],
) -> Result<HashMap<String, String>> {
    let now_unix = current_unix_timestamp();
    let mut trusted = HashMap::new();
    for root in trust_roots {
        let root_public_key = root.public_key.trim().to_string();
        let root_key_bytes =
            decode_base64_fixed::<32>("trusted root public key", &root_public_key)?;
        let _ = VerifyingKey::from_bytes(&root_key_bytes)
            .with_context(|| format!("invalid trusted root key '{}'", root.id))?;

        if let Some(existing) = trusted.get(&root.id) {
            if existing != &root_public_key {
                bail!(
                    "duplicate trusted root id '{}' has conflicting keys",
                    root.id
                );
            }
            continue;
        }
        trusted.insert(root.id.clone(), root_public_key);
    }

    let mut manifest_keys = HashMap::new();
    for key in &manifest.keys {
        if manifest_keys.insert(key.id.clone(), key).is_some() {
            bail!("registry contains duplicate key id '{}'", key.id);
        }
        if key.signed_by.is_some() ^ key.signature.is_some() {
            bail!(
                "registry key '{}' has incomplete signing metadata (signed_by and signature are required together)",
                key.id
            );
        }
    }

    for root in trust_roots {
        if let Some(manifest_root) = manifest_keys.get(&root.id) {
            if manifest_root.revoked {
                bail!("trusted root '{}' is revoked in registry manifest", root.id);
            }
            if is_expired(manifest_root.expires_unix, now_unix) {
                bail!("trusted root '{}' is expired in registry manifest", root.id);
            }
            if manifest_root.public_key.trim() != root.public_key.trim() {
                bail!(
                    "trusted root '{}' does not match registry key material",
                    root.id
                );
            }
        }
    }

    loop {
        let mut progressed = false;
        for key in &manifest.keys {
            if trusted.contains_key(&key.id) {
                continue;
            }
            if key.revoked || is_expired(key.expires_unix, now_unix) {
                continue;
            }

            let (Some(signed_by), Some(signature)) = (&key.signed_by, &key.signature) else {
                continue;
            };
            let Some(signer_public_key) = trusted.get(signed_by).cloned() else {
                continue;
            };

            verify_ed25519_signature(
                key_certificate_payload(key).as_bytes(),
                signature,
                &signer_public_key,
            )
            .with_context(|| {
                format!(
                    "failed to verify registry key '{}' signed by '{}'",
                    key.id, signed_by
                )
            })?;

            trusted.insert(key.id.clone(), key.public_key.trim().to_string());
            progressed = true;
        }

        if !progressed {
            break;
        }
    }

    Ok(trusted)
}

fn verify_ed25519_signature(
    message: &[u8],
    signature_base64: &str,
    public_key_base64: &str,
) -> Result<()> {
    let public_key_bytes = decode_base64_fixed::<32>("public key", public_key_base64)?;
    let signature_bytes = decode_base64_fixed::<64>("signature", signature_base64)?;
    let verifying_key = VerifyingKey::from_bytes(&public_key_bytes)
        .context("failed to decode ed25519 public key bytes")?;
    let signature = Signature::from_bytes(&signature_bytes);
    verifying_key
        .verify_strict(message, &signature)
        .map_err(|error| anyhow!("invalid ed25519 signature: {error}"))?;
    Ok(())
}

fn decode_base64_fixed<const N: usize>(label: &str, value: &str) -> Result<[u8; N]> {
    let decoded = BASE64
        .decode(value.trim())
        .with_context(|| format!("failed to decode {label} from base64"))?;
    let bytes: [u8; N] = decoded
        .try_into()
        .map_err(|_| anyhow!("{label} must decode to {N} bytes"))?;
    Ok(bytes)
}

fn key_certificate_payload(key: &RegistryKeyEntry) -> String {
    format!("tau-skill-key-v1:{}:{}", key.id, key.public_key.trim())
}

fn current_unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn is_expired(expires_unix: Option<u64>, now_unix: u64) -> bool {
    matches!(expires_unix, Some(expires_unix) if expires_unix <= now_unix)
}

fn upsert_skill_file(
    destination: &Path,
    content: &str,
    report: &mut SkillInstallReport,
) -> Result<()> {
    if destination.exists() {
        let existing = fs::read_to_string(destination)
            .with_context(|| format!("failed to read installed skill {}", destination.display()))?;
        if existing == content {
            report.skipped += 1;
            return Ok(());
        }

        fs::write(destination, content.as_bytes())
            .with_context(|| format!("failed to update skill {}", destination.display()))?;
        report.updated += 1;
        return Ok(());
    }

    fs::write(destination, content.as_bytes())
        .with_context(|| format!("failed to install skill {}", destination.display()))?;
    report.installed += 1;
    Ok(())
}

fn remote_skill_cache_path(cache_dir: &Path, source_url: &str) -> PathBuf {
    cache_dir
        .join(SKILLS_CACHE_ARTIFACTS_DIR)
        .join(format!("{}.bin", sha256_hex(source_url.as_bytes())))
}

fn registry_manifest_cache_path(cache_dir: &Path, registry_url: &str) -> PathBuf {
    cache_dir
        .join(SKILLS_CACHE_MANIFESTS_DIR)
        .join(format!("{}.json", sha256_hex(registry_url.as_bytes())))
}

fn read_cached_payload(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).with_context(|| format!("failed to read cache file {}", path.display()))
}

fn write_cached_payload(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create cache directory {}", parent.display()))?;
    }
    fs::write(path, bytes).with_context(|| format!("failed to write cache file {}", path.display()))
}

fn remote_skill_file_name(url: &Url, index: usize) -> String {
    let base_name = url
        .path_segments()
        .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
        .map(std::string::ToString::to_string)
        .unwrap_or_else(|| format!("remote-skill-{}", index + 1));

    if base_name.ends_with(".md") {
        base_name
    } else {
        format!("{base_name}.md")
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn signature_digest_sha256_hex(signature: &str) -> String {
    sha256_hex(signature.trim().as_bytes())
}

fn normalize_sha256(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
    use ed25519_dalek::{Signer, SigningKey};
    use httpmock::prelude::*;
    use sha2::{Digest, Sha256};
    use tempfile::tempdir;

    use super::{
        augment_system_prompt, build_local_skill_lock_hints, build_registry_skill_lock_hints,
        build_remote_skill_lock_hints, default_skills_cache_dir, default_skills_lock_path,
        fetch_registry_manifest_with_cache, install_remote_skills_with_cache, install_skills,
        load_catalog, load_skills_lockfile, remote_skill_file_name_for_source,
        resolve_registry_skill_sources, resolve_remote_skill_sources, resolve_selected_skills,
        sync_skills_with_lockfile, write_skills_lockfile, RegistryKeyEntry, RemoteSkillSource,
        Skill, SkillInstallReport, SkillLockSource, SkillRegistryManifest, SkillsDownloadOptions,
        TrustedKey,
    };

    async fn install_remote_skills(
        sources: &[RemoteSkillSource],
        destination_dir: &std::path::Path,
    ) -> anyhow::Result<SkillInstallReport> {
        install_remote_skills_with_cache(
            sources,
            destination_dir,
            &SkillsDownloadOptions::default(),
        )
        .await
    }

    async fn fetch_registry_manifest(
        registry_url: &str,
        expected_sha256: Option<&str>,
    ) -> anyhow::Result<SkillRegistryManifest> {
        fetch_registry_manifest_with_cache(
            registry_url,
            expected_sha256,
            &SkillsDownloadOptions::default(),
        )
        .await
    }

    #[test]
    fn unit_load_catalog_reads_markdown_files_only() {
        let temp = tempdir().expect("tempdir");
        std::fs::write(temp.path().join("a.md"), "A").expect("write a");
        std::fs::write(temp.path().join("b.txt"), "B").expect("write b");
        std::fs::write(temp.path().join("c.md"), "C").expect("write c");

        let catalog = load_catalog(temp.path()).expect("catalog");
        let names = catalog
            .iter()
            .map(|skill| skill.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["a", "c"]);
    }

    #[test]
    fn functional_augment_system_prompt_preserves_selected_skill_order() {
        let skills = vec![
            Skill {
                name: "first".to_string(),
                content: "one".to_string(),
                path: "first.md".into(),
            },
            Skill {
                name: "second".to_string(),
                content: "two".to_string(),
                path: "second.md".into(),
            },
        ];

        let prompt = augment_system_prompt("base", &skills);
        assert!(prompt.contains("# Skill: first\none"));
        assert!(prompt.contains("# Skill: second\ntwo"));
        assert!(prompt.find("first").expect("first") < prompt.find("second").expect("second"));
    }

    #[test]
    fn regression_resolve_selected_skills_errors_on_unknown_skill() {
        let catalog = vec![Skill {
            name: "known".to_string(),
            content: "x".to_string(),
            path: "known.md".into(),
        }];

        let error = resolve_selected_skills(&catalog, &["missing".to_string()])
            .expect_err("unknown skill should fail");
        assert!(error.to_string().contains("unknown skill 'missing'"));
    }

    #[test]
    fn integration_load_and_resolve_selected_skills_roundtrip() {
        let temp = tempdir().expect("tempdir");
        std::fs::write(temp.path().join("alpha.md"), "alpha body").expect("write alpha");
        std::fs::write(temp.path().join("beta.md"), "beta body").expect("write beta");

        let catalog = load_catalog(temp.path()).expect("catalog");
        let selected =
            resolve_selected_skills(&catalog, &["beta".to_string(), "alpha".to_string()])
                .expect("resolve");
        assert_eq!(
            selected
                .iter()
                .map(|skill| skill.name.as_str())
                .collect::<Vec<_>>(),
            vec!["beta", "alpha"]
        );
    }

    #[test]
    fn unit_install_skills_copies_new_skill_files() {
        let temp = tempdir().expect("tempdir");
        let source = temp.path().join("source.md");
        std::fs::write(&source, "source").expect("write source");
        let install_dir = temp.path().join("skills");

        let report = install_skills(&[source], &install_dir).expect("install");
        assert_eq!(
            report,
            SkillInstallReport {
                installed: 1,
                updated: 0,
                skipped: 0
            }
        );
        assert_eq!(
            std::fs::read_to_string(install_dir.join("source.md")).expect("read installed"),
            "source"
        );
    }

    #[test]
    fn regression_install_skills_skips_when_content_unchanged() {
        let temp = tempdir().expect("tempdir");
        let install_dir = temp.path().join("skills");
        std::fs::create_dir_all(&install_dir).expect("mkdir");
        std::fs::write(install_dir.join("stable.md"), "same").expect("write installed");

        let source = temp.path().join("stable.md");
        std::fs::write(&source, "same").expect("write source");

        let report = install_skills(&[source], &install_dir).expect("install");
        assert_eq!(
            report,
            SkillInstallReport {
                installed: 0,
                updated: 0,
                skipped: 1
            }
        );
    }

    #[test]
    fn integration_install_skills_updates_existing_content() {
        let temp = tempdir().expect("tempdir");
        let install_dir = temp.path().join("skills");
        std::fs::create_dir_all(&install_dir).expect("mkdir");
        std::fs::write(install_dir.join("evolve.md"), "v1").expect("write installed");

        let source = temp.path().join("evolve.md");
        std::fs::write(&source, "v2").expect("write source");

        let report = install_skills(&[PathBuf::from(&source)], &install_dir).expect("install");
        assert_eq!(
            report,
            SkillInstallReport {
                installed: 0,
                updated: 1,
                skipped: 0
            }
        );
        assert_eq!(
            std::fs::read_to_string(install_dir.join("evolve.md")).expect("read installed"),
            "v2"
        );
    }

    #[test]
    fn unit_default_skills_lock_path_appends_lockfile_name() {
        let root = PathBuf::from("skills");
        let lock_path = default_skills_lock_path(&root);
        assert_eq!(lock_path, PathBuf::from("skills").join("skills.lock.json"));
    }

    #[test]
    fn unit_default_skills_cache_dir_appends_cache_folder() {
        let root = PathBuf::from("skills");
        let cache_dir = default_skills_cache_dir(&root);
        assert_eq!(cache_dir, PathBuf::from("skills").join(".cache"));
    }

    #[test]
    fn functional_build_skill_lock_hints_capture_source_metadata() {
        assert_eq!(
            remote_skill_file_name_for_source("https://example.com/skills/path", 0)
                .expect("file name"),
            "path.md"
        );

        let local_hints =
            build_local_skill_lock_hints(&[PathBuf::from("/tmp/local-skill.md")]).expect("local");
        assert_eq!(local_hints.len(), 1);
        assert_eq!(local_hints[0].file, "local-skill.md");
        match &local_hints[0].source {
            SkillLockSource::Local { path } => assert!(path.ends_with("local-skill.md")),
            other => panic!("expected local source, got {other:?}"),
        }

        let remote_sources = vec![RemoteSkillSource {
            url: "https://example.com/skills/review".to_string(),
            sha256: Some("abc".to_string()),
            signing_key_id: Some("publisher".to_string()),
            signature: Some("sig".to_string()),
            signer_public_key: Some("key".to_string()),
        }];
        let remote_hints = build_remote_skill_lock_hints(&remote_sources).expect("remote");
        assert_eq!(remote_hints.len(), 1);
        assert_eq!(remote_hints[0].file, "review.md");
        match &remote_hints[0].source {
            SkillLockSource::Remote {
                url,
                expected_sha256,
                signing_key_id,
                signature,
                signer_public_key,
                signature_sha256,
            } => {
                assert_eq!(url, "https://example.com/skills/review");
                assert_eq!(expected_sha256.as_deref(), Some("abc"));
                assert_eq!(signing_key_id.as_deref(), Some("publisher"));
                assert_eq!(signature.as_deref(), Some("sig"));
                assert_eq!(signer_public_key.as_deref(), Some("key"));
                assert_eq!(
                    signature_sha256.as_deref(),
                    Some("a543997d84f12798350c09bdef2cdb171bf41ed3e4a5f808af2feb0c56263009")
                );
            }
            other => panic!("expected remote source, got {other:?}"),
        }

        let registry_hints = build_registry_skill_lock_hints(
            "https://registry.example.com/manifest.json",
            &["review".to_string()],
            &remote_sources,
        )
        .expect("registry");
        assert_eq!(registry_hints.len(), 1);
        match &registry_hints[0].source {
            SkillLockSource::Registry {
                registry_url,
                name,
                url,
                signing_key_id,
                signature_sha256,
                ..
            } => {
                assert_eq!(registry_url, "https://registry.example.com/manifest.json");
                assert_eq!(name, "review");
                assert_eq!(url, "https://example.com/skills/review");
                assert_eq!(signing_key_id.as_deref(), Some("publisher"));
                assert_eq!(
                    signature_sha256.as_deref(),
                    Some("a543997d84f12798350c09bdef2cdb171bf41ed3e4a5f808af2feb0c56263009")
                );
            }
            other => panic!("expected registry source, got {other:?}"),
        }
    }

    #[test]
    fn functional_write_and_load_skills_lockfile_roundtrip() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("local.md"), "local body").expect("write local");
        std::fs::write(skills_dir.join("remote.md"), "remote body").expect("write remote");

        let mut hints = build_local_skill_lock_hints(&[PathBuf::from("local.md")]).expect("local");
        hints.extend(
            build_remote_skill_lock_hints(&[RemoteSkillSource {
                url: "https://example.com/skills/remote.md".to_string(),
                sha256: Some("deadbeef".to_string()),
                signing_key_id: None,
                signature: None,
                signer_public_key: None,
            }])
            .expect("remote"),
        );
        let lock_path = default_skills_lock_path(&skills_dir);
        let written = write_skills_lockfile(&skills_dir, &lock_path, &hints).expect("write lock");
        assert_eq!(written.schema_version, 1);
        assert_eq!(written.entries.len(), 2);

        let loaded = load_skills_lockfile(&lock_path).expect("load lock");
        assert_eq!(loaded, written);
    }

    #[tokio::test]
    async fn integration_remote_install_lockfile_write_and_sync_succeeds() {
        let server = MockServer::start();
        let body = "remote lock skill";
        let checksum = format!("{:x}", Sha256::digest(body.as_bytes()));
        let remote = server.mock(|when, then| {
            when.method(GET).path("/skills/lock.md");
            then.status(200).body(body);
        });

        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        let sources = vec![RemoteSkillSource {
            url: format!("{}/skills/lock.md", server.base_url()),
            sha256: Some(checksum),
            signing_key_id: None,
            signature: None,
            signer_public_key: None,
        }];
        let report = install_remote_skills(&sources, &skills_dir)
            .await
            .expect("install remote");
        assert_eq!(report.installed, 1);

        let hints = build_remote_skill_lock_hints(&sources).expect("hints");
        let lock_path = default_skills_lock_path(&skills_dir);
        let lockfile = write_skills_lockfile(&skills_dir, &lock_path, &hints).expect("write lock");
        assert_eq!(lockfile.entries.len(), 1);
        assert_eq!(lockfile.entries[0].file, "lock.md");

        let sync = sync_skills_with_lockfile(&skills_dir, &lock_path).expect("sync");
        assert!(sync.in_sync());
        remote.assert_calls(1);
    }

    #[test]
    fn regression_sync_skills_with_lockfile_reports_missing_extra_and_changed_files() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("a.md"), "one").expect("write a");
        std::fs::write(skills_dir.join("b.md"), "two").expect("write b");

        let lock_path = default_skills_lock_path(&skills_dir);
        let lock = write_skills_lockfile(&skills_dir, &lock_path, &[]).expect("write lock");
        assert_eq!(lock.entries.len(), 2);

        std::fs::write(skills_dir.join("a.md"), "one changed").expect("update a");
        std::fs::remove_file(skills_dir.join("b.md")).expect("remove b");
        std::fs::write(skills_dir.join("c.md"), "three").expect("write c");

        let report = sync_skills_with_lockfile(&skills_dir, &lock_path).expect("sync");
        assert!(!report.in_sync());
        assert_eq!(report.changed, vec!["a.md".to_string()]);
        assert_eq!(report.missing, vec!["b.md".to_string()]);
        assert_eq!(report.extra, vec!["c.md".to_string()]);
        assert!(report.metadata_mismatch.is_empty());
    }

    #[test]
    fn regression_sync_skills_with_lockfile_reports_signature_metadata_mismatch() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("focus.md"), "secure body").expect("write skill");

        let actual_sha = format!("{:x}", Sha256::digest("secure body".as_bytes()));
        let lock_path = default_skills_lock_path(&skills_dir);
        std::fs::write(
            &lock_path,
            serde_json::json!({
                "schema_version": 1,
                "entries": [{
                    "name": "focus",
                    "file": "focus.md",
                    "sha256": actual_sha.clone(),
                    "source": {
                        "kind": "remote",
                        "url": "https://example.com/skills/focus.md",
                        "expected_sha256": actual_sha,
                        "signing_key_id": "publisher",
                        "signature": "sig",
                        "signer_public_key": "key",
                        "signature_sha256": "deadbeef"
                    }
                }]
            })
            .to_string(),
        )
        .expect("write lock");

        let report = sync_skills_with_lockfile(&skills_dir, &lock_path).expect("sync");
        assert!(!report.in_sync());
        assert!(report.changed.is_empty());
        assert!(report.missing.is_empty());
        assert!(report.extra.is_empty());
        assert_eq!(report.metadata_mismatch.len(), 1);
        assert!(
            report.metadata_mismatch[0].contains("signature_sha256"),
            "unexpected metadata mismatch payload: {}",
            report.metadata_mismatch[0]
        );
    }

    #[test]
    fn regression_load_skills_lockfile_rejects_unsupported_schema_version() {
        let temp = tempdir().expect("tempdir");
        let lock_path = temp.path().join("skills.lock.json");
        std::fs::write(
            &lock_path,
            serde_json::json!({
                "schema_version": 99,
                "entries": []
            })
            .to_string(),
        )
        .expect("write lock");

        let error = load_skills_lockfile(&lock_path).expect_err("unsupported schema should fail");
        assert!(error
            .to_string()
            .contains("unsupported skills lockfile schema_version"));
    }

    #[test]
    fn regression_resolve_remote_skill_sources_requires_matching_sha_count() {
        let error = resolve_remote_skill_sources(
            &["https://example.com/a.md".to_string()],
            &["abc".to_string(), "def".to_string()],
        )
        .expect_err("mismatched lengths should fail");
        assert!(error.to_string().contains("count"));
    }

    #[tokio::test]
    async fn functional_install_remote_skills_fetches_and_verifies_checksum() {
        let server = MockServer::start();
        let body = "remote skill body";
        let checksum = format!("{:x}", Sha256::digest(body.as_bytes()));

        let remote = server.mock(|when, then| {
            when.method(GET).path("/skills/review.md");
            then.status(200).body(body);
        });

        let temp = tempdir().expect("tempdir");
        let destination = temp.path().join("skills");
        let report = install_remote_skills(
            &[RemoteSkillSource {
                url: format!("{}/skills/review.md", server.base_url()),
                sha256: Some(checksum),
                signing_key_id: None,
                signature: None,
                signer_public_key: None,
            }],
            &destination,
        )
        .await
        .expect("remote install should succeed");

        assert_eq!(
            report,
            SkillInstallReport {
                installed: 1,
                updated: 0,
                skipped: 0
            }
        );
        assert_eq!(
            std::fs::read_to_string(destination.join("review.md")).expect("read installed"),
            body
        );
        remote.assert_calls(1);
    }

    #[tokio::test]
    async fn functional_install_remote_skills_with_cache_reuses_cached_payload() {
        let server = MockServer::start();
        let body = "cached remote skill";
        let checksum = format!("{:x}", Sha256::digest(body.as_bytes()));
        let remote = server.mock(|when, then| {
            when.method(GET).path("/skills/cache.md");
            then.status(200).body(body);
        });

        let temp = tempdir().expect("tempdir");
        let destination = temp.path().join("skills");
        let cache_dir = temp.path().join("cache");
        let source = RemoteSkillSource {
            url: format!("{}/skills/cache.md", server.base_url()),
            sha256: Some(checksum),
            signing_key_id: None,
            signature: None,
            signer_public_key: None,
        };

        let online = SkillsDownloadOptions {
            cache_dir: Some(cache_dir.clone()),
            offline: false,
        };
        let first =
            install_remote_skills_with_cache(std::slice::from_ref(&source), &destination, &online)
                .await
                .expect("first install");
        let second = install_remote_skills_with_cache(&[source], &destination, &online)
            .await
            .expect("second install");

        assert_eq!(first.installed, 1);
        assert_eq!(second.skipped, 1);
        remote.assert_calls(1);
    }

    #[tokio::test]
    async fn integration_install_remote_skills_with_cache_supports_offline_replay() {
        let server = MockServer::start();
        let body = "offline replay payload";
        let checksum = format!("{:x}", Sha256::digest(body.as_bytes()));
        let remote = server.mock(|when, then| {
            when.method(GET).path("/skills/offline.md");
            then.status(200).body(body);
        });

        let temp = tempdir().expect("tempdir");
        let destination = temp.path().join("skills");
        let cache_dir = temp.path().join("cache");
        let source = RemoteSkillSource {
            url: format!("{}/skills/offline.md", server.base_url()),
            sha256: Some(checksum),
            signing_key_id: None,
            signature: None,
            signer_public_key: None,
        };

        let online = SkillsDownloadOptions {
            cache_dir: Some(cache_dir.clone()),
            offline: false,
        };
        let offline = SkillsDownloadOptions {
            cache_dir: Some(cache_dir),
            offline: true,
        };

        install_remote_skills_with_cache(std::slice::from_ref(&source), &destination, &online)
            .await
            .expect("online warm cache");
        let report = install_remote_skills_with_cache(&[source], &destination, &offline)
            .await
            .expect("offline replay");

        assert_eq!(report.skipped, 1);
        remote.assert_calls(1);
    }

    #[tokio::test]
    async fn regression_install_remote_skills_with_cache_offline_reports_cache_miss() {
        let temp = tempdir().expect("tempdir");
        let destination = temp.path().join("skills");
        let cache_dir = temp.path().join("cache");
        let source = RemoteSkillSource {
            url: "https://example.com/skills/missing.md".to_string(),
            sha256: Some("deadbeef".to_string()),
            signing_key_id: None,
            signature: None,
            signer_public_key: None,
        };

        let options = SkillsDownloadOptions {
            cache_dir: Some(cache_dir),
            offline: true,
        };
        let error = install_remote_skills_with_cache(&[source], &destination, &options)
            .await
            .expect_err("offline without warm cache should fail");
        assert!(error
            .to_string()
            .contains("offline cache miss for skill URL"));
    }

    #[tokio::test]
    async fn regression_install_remote_skills_with_cache_refreshes_corrupt_cache_online() {
        let server = MockServer::start();
        let body = "fresh payload";
        let checksum = format!("{:x}", Sha256::digest(body.as_bytes()));
        let remote = server.mock(|when, then| {
            when.method(GET).path("/skills/fresh.md");
            then.status(200).body(body);
        });

        let temp = tempdir().expect("tempdir");
        let destination = temp.path().join("skills");
        let cache_dir = temp.path().join("cache");
        let source = RemoteSkillSource {
            url: format!("{}/skills/fresh.md", server.base_url()),
            sha256: Some(checksum),
            signing_key_id: None,
            signature: None,
            signer_public_key: None,
        };
        let cache_path = super::remote_skill_cache_path(&cache_dir, &source.url);
        std::fs::create_dir_all(cache_path.parent().expect("cache parent")).expect("mkdir cache");
        std::fs::write(&cache_path, b"corrupt").expect("write corrupt cache");

        let options = SkillsDownloadOptions {
            cache_dir: Some(cache_dir),
            offline: false,
        };
        let report = install_remote_skills_with_cache(&[source], &destination, &options)
            .await
            .expect("online install should recover cache");
        assert_eq!(report.installed, 1);
        assert_eq!(
            std::fs::read(&cache_path).expect("read cache"),
            body.as_bytes()
        );
        remote.assert_calls(1);
    }

    #[tokio::test]
    async fn regression_install_remote_skills_fails_on_checksum_mismatch() {
        let server = MockServer::start();
        let remote = server.mock(|when, then| {
            when.method(GET).path("/skills/check.md");
            then.status(200).body("payload");
        });

        let temp = tempdir().expect("tempdir");
        let destination = temp.path().join("skills");
        let error = install_remote_skills(
            &[RemoteSkillSource {
                url: format!("{}/skills/check.md", server.base_url()),
                sha256: Some("deadbeef".to_string()),
                signing_key_id: None,
                signature: None,
                signer_public_key: None,
            }],
            &destination,
        )
        .await
        .expect_err("checksum mismatch should fail");

        assert!(error.to_string().contains("sha256 mismatch"));
        remote.assert_calls(1);
    }

    #[tokio::test]
    async fn integration_install_remote_skills_updates_existing_file() {
        let server = MockServer::start();
        let remote = server.mock(|when, then| {
            when.method(GET).path("/skills/sync");
            then.status(200).body("v2");
        });

        let temp = tempdir().expect("tempdir");
        let destination = temp.path().join("skills");
        std::fs::create_dir_all(&destination).expect("mkdir");
        std::fs::write(destination.join("sync.md"), "v1").expect("write existing");

        let report = install_remote_skills(
            &[RemoteSkillSource {
                url: format!("{}/skills/sync", server.base_url()),
                sha256: None,
                signing_key_id: None,
                signature: None,
                signer_public_key: None,
            }],
            &destination,
        )
        .await
        .expect("remote update should succeed");

        assert_eq!(
            report,
            SkillInstallReport {
                installed: 0,
                updated: 1,
                skipped: 0
            }
        );
        assert_eq!(
            std::fs::read_to_string(destination.join("sync.md")).expect("read updated"),
            "v2"
        );
        remote.assert_calls(1);
    }

    #[tokio::test]
    async fn functional_fetch_registry_manifest_with_checksum_verification() {
        let server = MockServer::start();
        let body = serde_json::json!({
            "version": 1,
            "skills": [
                {"name":"review","url":"https://example.com/review.md","sha256":"abc"}
            ]
        })
        .to_string();
        let checksum = format!("{:x}", Sha256::digest(body.as_bytes()));

        let registry = server.mock(|when, then| {
            when.method(GET).path("/registry.json");
            then.status(200).body(body);
        });

        let manifest = fetch_registry_manifest(
            &format!("{}/registry.json", server.base_url()),
            Some(&checksum),
        )
        .await
        .expect("fetch should succeed");
        assert_eq!(manifest.version, 1);
        assert_eq!(manifest.skills.len(), 1);
        registry.assert_calls(1);
    }

    #[tokio::test]
    async fn integration_fetch_registry_manifest_with_cache_supports_offline_replay() {
        let server = MockServer::start();
        let body = serde_json::json!({
            "version": 1,
            "skills": [
                {"name":"cached","url":"https://example.com/cached.md","sha256":"abc"}
            ]
        })
        .to_string();
        let checksum = format!("{:x}", Sha256::digest(body.as_bytes()));
        let registry = server.mock(|when, then| {
            when.method(GET).path("/registry-cache.json");
            then.status(200).body(body);
        });

        let temp = tempdir().expect("tempdir");
        let cache_dir = temp.path().join("cache");
        let online = SkillsDownloadOptions {
            cache_dir: Some(cache_dir.clone()),
            offline: false,
        };
        let offline = SkillsDownloadOptions {
            cache_dir: Some(cache_dir),
            offline: true,
        };
        let url = format!("{}/registry-cache.json", server.base_url());

        let warm = fetch_registry_manifest_with_cache(&url, Some(&checksum), &online)
            .await
            .expect("warm cache");
        let replay = fetch_registry_manifest_with_cache(&url, Some(&checksum), &offline)
            .await
            .expect("offline replay");

        assert_eq!(warm, replay);
        registry.assert_calls(1);
    }

    #[tokio::test]
    async fn regression_fetch_registry_manifest_with_cache_offline_reports_cache_miss() {
        let temp = tempdir().expect("tempdir");
        let options = SkillsDownloadOptions {
            cache_dir: Some(temp.path().join("cache")),
            offline: true,
        };
        let error =
            fetch_registry_manifest_with_cache("https://example.com/registry.json", None, &options)
                .await
                .expect_err("offline cache miss should fail");
        assert!(error
            .to_string()
            .contains("offline cache miss for registry"));
    }

    #[tokio::test]
    async fn regression_fetch_registry_manifest_with_cache_refreshes_corrupt_cache_online() {
        let server = MockServer::start();
        let body = serde_json::json!({
            "version": 1,
            "skills": [{"name":"fresh","url":"https://example.com/fresh.md","sha256":"abc"}]
        })
        .to_string();
        let checksum = format!("{:x}", Sha256::digest(body.as_bytes()));
        let registry = server.mock(|when, then| {
            when.method(GET).path("/registry-fresh.json");
            then.status(200).body(body);
        });

        let temp = tempdir().expect("tempdir");
        let cache_dir = temp.path().join("cache");
        let url = format!("{}/registry-fresh.json", server.base_url());
        let cache_path = super::registry_manifest_cache_path(&cache_dir, &url);
        std::fs::create_dir_all(cache_path.parent().expect("cache parent")).expect("mkdir cache");
        std::fs::write(&cache_path, b"corrupt").expect("write corrupt cache");
        let options = SkillsDownloadOptions {
            cache_dir: Some(cache_dir),
            offline: false,
        };

        let manifest = fetch_registry_manifest_with_cache(&url, Some(&checksum), &options)
            .await
            .expect("online fetch should recover cache");
        assert_eq!(manifest.version, 1);
        assert_eq!(
            std::fs::read_to_string(cache_path).expect("read cache"),
            serde_json::json!({
                "version": 1,
                "skills": [{"name":"fresh","url":"https://example.com/fresh.md","sha256":"abc"}]
            })
            .to_string()
        );
        registry.assert_calls(1);
    }

    #[test]
    fn regression_resolve_registry_skill_sources_errors_for_missing_name() {
        let manifest = SkillRegistryManifest {
            version: 1,
            keys: vec![],
            skills: vec![super::RegistrySkillEntry {
                name: "known".to_string(),
                url: "https://example.com/known.md".to_string(),
                sha256: None,
                signing_key: None,
                signature: None,
                revoked: false,
                expires_unix: None,
            }],
        };

        let error = resolve_registry_skill_sources(&manifest, &["missing".to_string()], &[], false)
            .expect_err("unknown name should fail");
        assert!(error
            .to_string()
            .contains("registry does not contain skill"));
    }

    #[tokio::test]
    async fn integration_registry_manifest_to_remote_install_roundtrip() {
        let server = MockServer::start();
        let skill_body = "from registry";
        let skill_sha = format!("{:x}", Sha256::digest(skill_body.as_bytes()));
        let registry_body = serde_json::json!({
            "version": 1,
            "skills": [
                {
                    "name":"registry-skill",
                    "url": format!("{}/skill.md", server.base_url()),
                    "sha256": skill_sha
                }
            ]
        })
        .to_string();

        let registry = server.mock(|when, then| {
            when.method(GET).path("/registry.json");
            then.status(200).body(registry_body);
        });
        let skill = server.mock(|when, then| {
            when.method(GET).path("/skill.md");
            then.status(200).body(skill_body);
        });

        let manifest =
            fetch_registry_manifest(&format!("{}/registry.json", server.base_url()), None)
                .await
                .expect("manifest fetch");
        let sources =
            resolve_registry_skill_sources(&manifest, &["registry-skill".to_string()], &[], false)
                .expect("resolve");

        let temp = tempdir().expect("tempdir");
        let destination = temp.path().join("skills");
        let report = install_remote_skills(&sources, &destination)
            .await
            .expect("install");

        assert_eq!(
            report,
            SkillInstallReport {
                installed: 1,
                updated: 0,
                skipped: 0
            }
        );
        assert_eq!(
            std::fs::read_to_string(destination.join("skill.md")).expect("read skill"),
            skill_body
        );
        registry.assert_calls(1);
        skill.assert_calls(1);
    }

    #[tokio::test]
    async fn regression_install_remote_skills_fails_on_signature_mismatch() {
        let server = MockServer::start();
        let body = "signed payload";
        let signer = SigningKey::from_bytes(&[7u8; 32]);
        let different_signer = SigningKey::from_bytes(&[8u8; 32]);
        let signature = BASE64.encode(different_signer.sign(body.as_bytes()).to_bytes());
        let public_key = BASE64.encode(signer.verifying_key().to_bytes());

        let remote = server.mock(|when, then| {
            when.method(GET).path("/skills/signed.md");
            then.status(200).body(body);
        });

        let temp = tempdir().expect("tempdir");
        let destination = temp.path().join("skills");
        let error = install_remote_skills(
            &[RemoteSkillSource {
                url: format!("{}/skills/signed.md", server.base_url()),
                sha256: None,
                signing_key_id: None,
                signature: Some(signature),
                signer_public_key: Some(public_key),
            }],
            &destination,
        )
        .await
        .expect_err("signature mismatch should fail");
        assert!(error
            .to_string()
            .contains("signature verification failed for"));
        remote.assert_calls(1);
    }

    #[tokio::test]
    async fn functional_registry_signed_skill_roundtrip_with_trust_chain() {
        let server = MockServer::start();

        let root = SigningKey::from_bytes(&[11u8; 32]);
        let publisher = SigningKey::from_bytes(&[12u8; 32]);
        let root_public_key = BASE64.encode(root.verifying_key().to_bytes());
        let publisher_public_key = BASE64.encode(publisher.verifying_key().to_bytes());
        let publisher_certificate = BASE64.encode(
            root.sign(format!("tau-skill-key-v1:publisher:{publisher_public_key}").as_bytes())
                .to_bytes(),
        );

        let skill_body = "signed registry skill";
        let skill_sha = format!("{:x}", Sha256::digest(skill_body.as_bytes()));
        let skill_signature = BASE64.encode(publisher.sign(skill_body.as_bytes()).to_bytes());

        let registry_body = serde_json::json!({
            "version": 1,
            "keys": [
                {
                    "id":"publisher",
                    "public_key": publisher_public_key,
                    "signed_by":"root",
                    "signature": publisher_certificate
                }
            ],
            "skills": [
                {
                    "name":"secure-skill",
                    "url": format!("{}/skills/secure.md", server.base_url()),
                    "sha256": skill_sha,
                    "signing_key":"publisher",
                    "signature": skill_signature
                }
            ]
        })
        .to_string();

        let registry = server.mock(|when, then| {
            when.method(GET).path("/registry.json");
            then.status(200).body(registry_body);
        });
        let skill = server.mock(|when, then| {
            when.method(GET).path("/skills/secure.md");
            then.status(200).body(skill_body);
        });

        let manifest =
            fetch_registry_manifest(&format!("{}/registry.json", server.base_url()), None)
                .await
                .expect("manifest fetch");
        let sources = resolve_registry_skill_sources(
            &manifest,
            &["secure-skill".to_string()],
            &[TrustedKey {
                id: "root".to_string(),
                public_key: root_public_key,
            }],
            true,
        )
        .expect("resolve signed sources");
        assert_eq!(sources[0].signing_key_id.as_deref(), Some("publisher"));

        let temp = tempdir().expect("tempdir");
        let destination = temp.path().join("skills");
        let report = install_remote_skills(&sources, &destination)
            .await
            .expect("install signed skill");

        assert_eq!(
            report,
            SkillInstallReport {
                installed: 1,
                updated: 0,
                skipped: 0
            }
        );
        assert_eq!(
            std::fs::read_to_string(destination.join("secure.md")).expect("read skill"),
            skill_body
        );
        let lock_hints = build_registry_skill_lock_hints(
            &format!("{}/registry.json", server.base_url()),
            &["secure-skill".to_string()],
            &sources,
        )
        .expect("build lock hints");
        let lock_path = default_skills_lock_path(&destination);
        let lock =
            write_skills_lockfile(&destination, &lock_path, &lock_hints).expect("write lock");
        assert_eq!(lock.entries.len(), 1);
        let expected_signature_sha =
            format!("{:x}", Sha256::digest(skill_signature.trim().as_bytes()));
        match &lock.entries[0].source {
            SkillLockSource::Registry {
                signing_key_id,
                signature_sha256,
                ..
            } => {
                assert_eq!(signing_key_id.as_deref(), Some("publisher"));
                assert_eq!(
                    signature_sha256.as_deref(),
                    Some(expected_signature_sha.as_str())
                );
            }
            other => panic!("expected registry lock source, got {other:?}"),
        }
        let sync = sync_skills_with_lockfile(&destination, &lock_path).expect("sync");
        assert!(sync.in_sync());
        registry.assert_calls(1);
        skill.assert_calls(1);
    }

    #[test]
    fn regression_resolve_registry_skill_sources_rejects_untrusted_signing_key() {
        let publisher = SigningKey::from_bytes(&[21u8; 32]);
        let manifest = SkillRegistryManifest {
            version: 1,
            keys: vec![],
            skills: vec![super::RegistrySkillEntry {
                name: "secure".to_string(),
                url: "https://example.com/secure.md".to_string(),
                sha256: None,
                signing_key: Some("publisher".to_string()),
                signature: Some(BASE64.encode(publisher.sign(b"payload").to_bytes())),
                revoked: false,
                expires_unix: None,
            }],
        };

        let error = resolve_registry_skill_sources(&manifest, &["secure".to_string()], &[], false)
            .expect_err("untrusted key should fail");
        assert!(error.to_string().contains("untrusted signing key"));
    }

    #[test]
    fn regression_resolve_registry_skill_sources_requires_signed_when_enabled() {
        let manifest = SkillRegistryManifest {
            version: 1,
            keys: vec![],
            skills: vec![super::RegistrySkillEntry {
                name: "plain".to_string(),
                url: "https://example.com/plain.md".to_string(),
                sha256: None,
                signing_key: None,
                signature: None,
                revoked: false,
                expires_unix: None,
            }],
        };

        let error = resolve_registry_skill_sources(&manifest, &["plain".to_string()], &[], true)
            .expect_err("unsigned entry should fail");
        assert!(error.to_string().contains("unsigned"));
    }

    #[test]
    fn regression_resolve_registry_skill_sources_rejects_invalid_key_certificate() {
        let root = SigningKey::from_bytes(&[31u8; 32]);
        let root_public_key = BASE64.encode(root.verifying_key().to_bytes());
        let publisher = SigningKey::from_bytes(&[32u8; 32]);
        let publisher_public_key = BASE64.encode(publisher.verifying_key().to_bytes());
        let invalid_certificate = BASE64.encode(publisher.sign(b"wrong payload").to_bytes());

        let manifest = SkillRegistryManifest {
            version: 1,
            keys: vec![RegistryKeyEntry {
                id: "publisher".to_string(),
                public_key: publisher_public_key,
                signed_by: Some("root".to_string()),
                signature: Some(invalid_certificate),
                revoked: false,
                expires_unix: None,
            }],
            skills: vec![super::RegistrySkillEntry {
                name: "secure".to_string(),
                url: "https://example.com/secure.md".to_string(),
                sha256: None,
                signing_key: Some("publisher".to_string()),
                signature: Some(BASE64.encode(publisher.sign(b"payload").to_bytes())),
                revoked: false,
                expires_unix: None,
            }],
        };

        let error = resolve_registry_skill_sources(
            &manifest,
            &["secure".to_string()],
            &[TrustedKey {
                id: "root".to_string(),
                public_key: root_public_key,
            }],
            true,
        )
        .expect_err("invalid certificate should fail");
        assert!(error.to_string().contains("failed to verify registry key"));
    }

    #[test]
    fn regression_resolve_registry_skill_sources_rejects_revoked_skill() {
        let manifest = SkillRegistryManifest {
            version: 1,
            keys: vec![],
            skills: vec![super::RegistrySkillEntry {
                name: "revoked".to_string(),
                url: "https://example.com/revoked.md".to_string(),
                sha256: None,
                signing_key: None,
                signature: None,
                revoked: true,
                expires_unix: None,
            }],
        };

        let error = resolve_registry_skill_sources(&manifest, &["revoked".to_string()], &[], false)
            .expect_err("revoked skill should fail");
        assert!(error.to_string().contains("is revoked"));
    }

    #[test]
    fn regression_resolve_registry_skill_sources_rejects_expired_skill() {
        let now = super::current_unix_timestamp();
        let manifest = SkillRegistryManifest {
            version: 1,
            keys: vec![],
            skills: vec![super::RegistrySkillEntry {
                name: "expired".to_string(),
                url: "https://example.com/expired.md".to_string(),
                sha256: None,
                signing_key: None,
                signature: None,
                revoked: false,
                expires_unix: Some(now.saturating_sub(1)),
            }],
        };

        let error = resolve_registry_skill_sources(&manifest, &["expired".to_string()], &[], false)
            .expect_err("expired skill should fail");
        assert!(error.to_string().contains("is expired"));
    }

    #[test]
    fn regression_resolve_registry_skill_sources_rejects_revoked_signing_key() {
        let root = SigningKey::from_bytes(&[41u8; 32]);
        let root_public_key = BASE64.encode(root.verifying_key().to_bytes());
        let publisher = SigningKey::from_bytes(&[42u8; 32]);
        let publisher_public_key = BASE64.encode(publisher.verifying_key().to_bytes());
        let cert = BASE64.encode(
            root.sign(format!("tau-skill-key-v1:publisher:{publisher_public_key}").as_bytes())
                .to_bytes(),
        );

        let manifest = SkillRegistryManifest {
            version: 1,
            keys: vec![RegistryKeyEntry {
                id: "publisher".to_string(),
                public_key: publisher_public_key,
                signed_by: Some("root".to_string()),
                signature: Some(cert),
                revoked: true,
                expires_unix: None,
            }],
            skills: vec![super::RegistrySkillEntry {
                name: "secure".to_string(),
                url: "https://example.com/secure.md".to_string(),
                sha256: None,
                signing_key: Some("publisher".to_string()),
                signature: Some(BASE64.encode(publisher.sign(b"payload").to_bytes())),
                revoked: false,
                expires_unix: None,
            }],
        };

        let error = resolve_registry_skill_sources(
            &manifest,
            &["secure".to_string()],
            &[TrustedKey {
                id: "root".to_string(),
                public_key: root_public_key,
            }],
            true,
        )
        .expect_err("revoked signing key should fail");
        assert!(error.to_string().contains("revoked signing key"));
    }
}
