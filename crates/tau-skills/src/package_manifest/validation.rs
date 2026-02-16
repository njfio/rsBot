use std::{
    path::{Component, PathBuf},
    str::FromStr,
};

use anyhow::{anyhow, bail, Context, Result};
use reqwest::Url;

use super::PackageComponent;

/// Validate one component set for non-empty ids, safe paths, and optional source metadata.
pub(super) fn validate_component_set(kind: &str, components: &[PackageComponent]) -> Result<()> {
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

/// Validate component path is relative and does not escape package root.
pub(super) fn validate_relative_component_path(kind: &str, id: &str, raw_path: &str) -> Result<()> {
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

/// Parse component source URL and enforce http/https schemes.
pub(super) fn parse_component_source_url(kind: &str, id: &str, raw_url: &str) -> Result<Url> {
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

/// Parse sha256 checksum in canonical 64-hex format.
pub(super) fn parse_sha256_checksum(raw_checksum: &str) -> Result<String> {
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

pub(super) fn is_semver_like(raw: &str) -> bool {
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
