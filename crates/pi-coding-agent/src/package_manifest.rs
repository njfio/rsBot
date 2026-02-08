use std::{
    path::{Component, Path, PathBuf},
    str::FromStr,
};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::Cli;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PackageManifest {
    schema_version: u32,
    name: String,
    version: String,
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

pub(crate) fn validate_package_manifest(path: &Path) -> Result<PackageManifestSummary> {
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

    Ok(PackageManifestSummary {
        manifest_path: path.to_path_buf(),
        name: name.to_string(),
        version: manifest.version.trim().to_string(),
        template_count: manifest.templates.len(),
        skill_count: manifest.skills.len(),
        extension_count: manifest.extensions.len(),
        theme_count: manifest.themes.len(),
        total_components,
    })
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
    use tempfile::tempdir;

    use super::validate_package_manifest;

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
    }
}
