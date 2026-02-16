//! Skill package management, trust, and verification runtime for Tau.
//!
//! Handles skill manifest parsing, install/update/remove, lockfile sync, trust
//! records, and signature verification workflows.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

mod commands;
mod load_registry;
mod package_manifest;
mod trust_policy;
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
    load_registry::load_catalog(dir)
}

pub fn install_skills(sources: &[PathBuf], destination_dir: &Path) -> Result<SkillInstallReport> {
    load_registry::install_skills(sources, destination_dir)
}

pub fn resolve_remote_skill_sources(
    urls: &[String],
    sha256_values: &[String],
) -> Result<Vec<RemoteSkillSource>> {
    load_registry::resolve_remote_skill_sources(urls, sha256_values)
}

pub async fn install_remote_skills_with_cache(
    sources: &[RemoteSkillSource],
    destination_dir: &Path,
    options: &SkillsDownloadOptions,
) -> Result<SkillInstallReport> {
    load_registry::install_remote_skills_with_cache(sources, destination_dir, options).await
}

pub fn default_skills_lock_path(skills_dir: &Path) -> PathBuf {
    load_registry::default_skills_lock_path(skills_dir)
}

pub fn default_skills_cache_dir(skills_dir: &Path) -> PathBuf {
    load_registry::default_skills_cache_dir(skills_dir)
}

pub fn build_local_skill_lock_hints(sources: &[PathBuf]) -> Result<Vec<SkillLockHint>> {
    load_registry::build_local_skill_lock_hints(sources)
}

pub fn build_remote_skill_lock_hints(sources: &[RemoteSkillSource]) -> Result<Vec<SkillLockHint>> {
    load_registry::build_remote_skill_lock_hints(sources)
}

pub fn build_registry_skill_lock_hints(
    registry_url: &str,
    names: &[String],
    sources: &[RemoteSkillSource],
) -> Result<Vec<SkillLockHint>> {
    load_registry::build_registry_skill_lock_hints(registry_url, names, sources)
}

pub fn remote_skill_file_name_for_source(url: &str, index: usize) -> Result<String> {
    load_registry::remote_skill_file_name_for_source(url, index)
}

pub fn load_skills_lockfile(path: &Path) -> Result<SkillsLockfile> {
    load_registry::load_skills_lockfile(path)
}

pub fn write_skills_lockfile(
    skills_dir: &Path,
    lock_path: &Path,
    hints: &[SkillLockHint],
) -> Result<SkillsLockfile> {
    load_registry::write_skills_lockfile(skills_dir, lock_path, hints)
}

pub fn sync_skills_with_lockfile(skills_dir: &Path, lock_path: &Path) -> Result<SkillsSyncReport> {
    load_registry::sync_skills_with_lockfile(skills_dir, lock_path)
}

pub async fn fetch_registry_manifest_with_cache(
    registry_url: &str,
    expected_sha256: Option<&str>,
    options: &SkillsDownloadOptions,
) -> Result<SkillRegistryManifest> {
    load_registry::fetch_registry_manifest_with_cache(registry_url, expected_sha256, options).await
}

pub fn resolve_registry_skill_sources(
    manifest: &SkillRegistryManifest,
    selected_names: &[String],
    trust_roots: &[TrustedKey],
    require_signed: bool,
) -> Result<Vec<RemoteSkillSource>> {
    load_registry::resolve_registry_skill_sources(
        manifest,
        selected_names,
        trust_roots,
        require_signed,
    )
}

pub fn resolve_selected_skills(catalog: &[Skill], selected: &[String]) -> Result<Vec<Skill>> {
    load_registry::resolve_selected_skills(catalog, selected)
}

pub fn augment_system_prompt(base: &str, skills: &[Skill]) -> String {
    load_registry::augment_system_prompt(base, skills)
}

#[cfg(test)]
fn current_unix_timestamp() -> u64 {
    trust_policy::current_unix_timestamp()
}

#[cfg(test)]
fn remote_skill_cache_path(cache_dir: &Path, source_url: &str) -> PathBuf {
    load_registry::remote_skill_cache_path(cache_dir, source_url)
}

#[cfg(test)]
fn registry_manifest_cache_path(cache_dir: &Path, registry_url: &str) -> PathBuf {
    load_registry::registry_manifest_cache_path(cache_dir, registry_url)
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
