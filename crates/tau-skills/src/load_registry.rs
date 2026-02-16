//! Skill loading, registry resolution, cache, and lockfile orchestration.

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};
use reqwest::Url;

use super::trust_policy::{
    build_trusted_key_map, current_unix_timestamp, is_expired, normalize_sha256, sha256_hex,
    signature_digest_sha256_hex, validate_remote_skill_payload,
    validate_remote_skill_source_metadata,
};
use super::{
    RemoteSkillSource, Skill, SkillInstallReport, SkillLockEntry, SkillLockHint, SkillLockSource,
    SkillRegistryManifest, SkillsDownloadOptions, SkillsLockfile, SkillsSyncReport, TrustedKey,
    SKILLS_CACHE_ARTIFACTS_DIR, SKILLS_CACHE_DIR_NAME, SKILLS_CACHE_MANIFESTS_DIR,
    SKILLS_LOCK_FILE_NAME, SKILLS_LOCK_SCHEMA_VERSION,
};

pub(super) fn load_catalog(dir: &Path) -> Result<Vec<Skill>> {
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

pub(super) fn install_skills(
    sources: &[PathBuf],
    destination_dir: &Path,
) -> Result<SkillInstallReport> {
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
            .ok_or_else(|| anyhow!("invalid skill source '{}'", source.display()))?;
        let destination = destination_dir.join(file_name);

        let content = fs::read_to_string(source)
            .with_context(|| format!("failed to read skill source {}", source.display()))?;
        upsert_skill_file(&destination, &content, &mut report)?;
    }

    Ok(report)
}

pub(super) fn resolve_remote_skill_sources(
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

pub(super) async fn install_remote_skills_with_cache(
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

fn should_reuse_cached_remote_payload(source: &RemoteSkillSource) -> bool {
    source.sha256.is_some() || source.signature.is_some()
}

pub(super) fn default_skills_lock_path(skills_dir: &Path) -> PathBuf {
    skills_dir.join(SKILLS_LOCK_FILE_NAME)
}

pub(super) fn default_skills_cache_dir(skills_dir: &Path) -> PathBuf {
    skills_dir.join(SKILLS_CACHE_DIR_NAME)
}

pub(super) fn build_local_skill_lock_hints(sources: &[PathBuf]) -> Result<Vec<SkillLockHint>> {
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

pub(super) fn build_remote_skill_lock_hints(
    sources: &[RemoteSkillSource],
) -> Result<Vec<SkillLockHint>> {
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

pub(super) fn build_registry_skill_lock_hints(
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

pub(super) fn remote_skill_file_name_for_source(url: &str, index: usize) -> Result<String> {
    let url = Url::parse(url).with_context(|| format!("invalid skill URL '{}'", url))?;
    Ok(remote_skill_file_name(&url, index))
}

pub(super) fn load_skills_lockfile(path: &Path) -> Result<SkillsLockfile> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read skills lockfile {}", path.display()))?;
    let lockfile: SkillsLockfile = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse skills lockfile {}", path.display()))?;
    validate_skills_lockfile(&lockfile, path)?;
    Ok(lockfile)
}

pub(super) fn write_skills_lockfile(
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

pub(super) fn sync_skills_with_lockfile(
    skills_dir: &Path,
    lock_path: &Path,
) -> Result<SkillsSyncReport> {
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

pub(super) async fn fetch_registry_manifest_with_cache(
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

pub(super) fn resolve_registry_skill_sources(
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

pub(super) fn resolve_selected_skills(
    catalog: &[Skill],
    selected: &[String],
) -> Result<Vec<Skill>> {
    let mut resolved = Vec::new();
    for name in selected {
        let skill = catalog
            .iter()
            .find(|skill| skill.name == *name)
            .cloned()
            .ok_or_else(|| anyhow!("unknown skill '{}'", name))?;
        resolved.push(skill);
    }

    Ok(resolved)
}

pub(super) fn augment_system_prompt(base: &str, skills: &[Skill]) -> String {
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

pub(super) fn remote_skill_cache_path(cache_dir: &Path, source_url: &str) -> PathBuf {
    cache_dir
        .join(SKILLS_CACHE_ARTIFACTS_DIR)
        .join(format!("{}.bin", sha256_hex(source_url.as_bytes())))
}

pub(super) fn registry_manifest_cache_path(cache_dir: &Path, registry_url: &str) -> PathBuf {
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
