use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{Signature, VerifyingKey};
use reqwest::Url;
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub content: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillInstallReport {
    pub installed: usize,
    pub updated: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteSkillSource {
    pub url: String,
    pub sha256: Option<String>,
    pub signature: Option<String>,
    pub signer_public_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedKey {
    pub id: String,
    pub public_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
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
            signature: None,
            signer_public_key: None,
        })
        .collect())
}

pub async fn install_remote_skills(
    sources: &[RemoteSkillSource],
    destination_dir: &Path,
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
            .with_context(|| format!("failed to read skill response '{}'", source.url))?;
        if let Some(expected_sha256) = &source.sha256 {
            let actual_sha256 = sha256_hex(&bytes);
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
        if source.signature.is_some() ^ source.signer_public_key.is_some() {
            bail!(
                "incomplete signature metadata for '{}': both signature and signer public key are required",
                source.url
            );
        }
        if let (Some(signature), Some(signer_public_key)) =
            (&source.signature, &source.signer_public_key)
        {
            verify_ed25519_signature(bytes.as_ref(), signature, signer_public_key)
                .with_context(|| format!("signature verification failed for '{}'", source.url))?;
        }

        let file_name = remote_skill_file_name(&url, index);
        let destination = destination_dir.join(file_name);
        let content = String::from_utf8(bytes.to_vec())
            .with_context(|| format!("skill content from '{}' is not UTF-8", source.url))?;

        upsert_skill_file(&destination, &content, &mut report)?;
    }

    Ok(report)
}

pub async fn fetch_registry_manifest(
    registry_url: &str,
    expected_sha256: Option<&str>,
) -> Result<SkillRegistryManifest> {
    let url = Url::parse(registry_url)
        .with_context(|| format!("invalid registry URL '{}'", registry_url))?;
    if !matches!(url.scheme(), "http" | "https") {
        bail!("unsupported registry URL scheme '{}'", url.scheme());
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
        .with_context(|| format!("failed to read registry response '{}'", registry_url))?;

    if let Some(expected_sha256) = expected_sha256 {
        let actual_sha256 = sha256_hex(&bytes);
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

    let manifest = serde_json::from_slice::<SkillRegistryManifest>(&bytes)
        .with_context(|| format!("failed to parse registry '{}'", registry_url))?;
    if manifest.version == 0 {
        bail!("registry '{}' has invalid version 0", registry_url);
    }
    Ok(manifest)
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
    format!("pi-skill-key-v1:{}:{}", key.id, key.public_key.trim())
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

fn remote_skill_file_name(url: &Url, index: usize) -> String {
    let base_name = url
        .path_segments()
        .and_then(|segments| {
            segments
                .filter(|segment| !segment.is_empty())
                .next_back()
                .map(|segment| segment.to_string())
        })
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
        augment_system_prompt, fetch_registry_manifest, install_remote_skills, install_skills,
        load_catalog, resolve_registry_skill_sources, resolve_remote_skill_sources,
        resolve_selected_skills, RegistryKeyEntry, RemoteSkillSource, Skill, SkillInstallReport,
        SkillRegistryManifest, TrustedKey,
    };

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
            root.sign(format!("pi-skill-key-v1:publisher:{publisher_public_key}").as_bytes())
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
            root.sign(format!("pi-skill-key-v1:publisher:{publisher_public_key}").as_bytes())
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
