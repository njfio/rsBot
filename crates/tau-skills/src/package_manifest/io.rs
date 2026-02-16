use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use reqwest::Url;

use super::{parse_component_source_url, verify_component_checksum, PackageComponent};

pub(super) fn resolve_component_source_path(
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

pub(super) fn resolve_component_source_bytes(
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
