//! Release-channel lookup, cache, and command runtime for Tau.
//!
//! Defines release metadata/cache models and runtime commands used by doctor
//! and startup workflows for release awareness.

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tau_core::write_text_atomic;

pub mod cache;
pub mod command_runtime;

pub const RELEASE_LOOKUP_CACHE_SCHEMA_VERSION: u32 = 1;
pub const RELEASE_LOOKUP_CACHE_TTL_MS: u64 = 15 * 60 * 1_000;
pub const RELEASE_CHANNEL_SCHEMA_VERSION: u32 = 2;
pub const RELEASE_LOOKUP_URL: &str = "https://api.github.com/repos/njfio/Tau/releases?per_page=30";
pub const RELEASE_LOOKUP_USER_AGENT: &str = "tau-coding-agent/release-channel-check";
pub const RELEASE_LOOKUP_TIMEOUT_MS: u64 = 8_000;

pub use command_runtime::{execute_release_channel_command, RELEASE_CHANNEL_USAGE};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
/// Enumerates supported `ReleaseChannel` values.
pub enum ReleaseChannel {
    Stable,
    Beta,
    Dev,
}

impl ReleaseChannel {
    pub fn as_str(self) -> &'static str {
        match self {
            ReleaseChannel::Stable => "stable",
            ReleaseChannel::Beta => "beta",
            ReleaseChannel::Dev => "dev",
        }
    }
}

impl std::fmt::Display for ReleaseChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ReleaseChannel {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "stable" => Ok(Self::Stable),
            "beta" => Ok(Self::Beta),
            "dev" => Ok(Self::Dev),
            _ => bail!(
                "invalid release channel '{}'; expected stable|beta|dev",
                value
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
/// Public struct `ReleaseChannelRollbackMetadata` used across Tau components.
pub struct ReleaseChannelRollbackMetadata {
    #[serde(default)]
    pub previous_channel: Option<ReleaseChannel>,
    #[serde(default)]
    pub previous_version: Option<String>,
    #[serde(default)]
    pub reference_unix_ms: Option<u64>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `ReleaseChannelStoreFile` used across Tau components.
pub struct ReleaseChannelStoreFile {
    pub schema_version: u32,
    pub release_channel: ReleaseChannel,
    #[serde(default)]
    pub rollback: ReleaseChannelRollbackMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct LegacyReleaseChannelStoreFile {
    schema_version: u32,
    release_channel: ReleaseChannel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `GitHubReleaseRecord` used across Tau components.
pub struct GitHubReleaseRecord {
    pub tag_name: String,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub draft: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `ReleaseLookupSource` values.
pub enum ReleaseLookupSource {
    Live,
    CacheFresh,
    CacheStaleFallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `LatestChannelReleaseResolution` used across Tau components.
pub struct LatestChannelReleaseResolution {
    pub latest: Option<String>,
    pub source: ReleaseLookupSource,
}

pub fn release_lookup_url() -> &'static str {
    RELEASE_LOOKUP_URL
}

pub fn default_release_channel_path() -> Result<PathBuf> {
    Ok(std::env::current_dir()
        .context("failed to resolve current working directory")?
        .join(".tau")
        .join("release-channel.json"))
}

pub fn load_release_channel_store(path: &Path) -> Result<Option<ReleaseChannel>> {
    Ok(load_release_channel_store_file(path)?.map(|store| store.release_channel))
}

pub fn load_release_channel_store_file(path: &Path) -> Result<Option<ReleaseChannelStoreFile>> {
    if !path.exists() {
        return Ok(None);
    }

    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read release channel file {}", path.display()))?;
    let value = serde_json::from_str::<serde_json::Value>(&raw)
        .with_context(|| format!("failed to parse release channel file {}", path.display()))?;
    let schema_version = value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            anyhow!(
                "release channel file {} is missing schema_version",
                path.display()
            )
        })?;

    match schema_version {
        1 => {
            let legacy = serde_json::from_value::<LegacyReleaseChannelStoreFile>(value)
                .with_context(|| {
                    format!("failed to parse release channel file {}", path.display())
                })?;
            Ok(Some(ReleaseChannelStoreFile {
                schema_version: RELEASE_CHANNEL_SCHEMA_VERSION,
                release_channel: legacy.release_channel,
                rollback: ReleaseChannelRollbackMetadata::default(),
            }))
        }
        version if version == RELEASE_CHANNEL_SCHEMA_VERSION as u64 => {
            let parsed =
                serde_json::from_value::<ReleaseChannelStoreFile>(value).with_context(|| {
                    format!("failed to parse release channel file {}", path.display())
                })?;
            Ok(Some(parsed))
        }
        other => bail!(
            "unsupported release channel schema_version {} in {} (expected {} or 1)",
            other,
            path.display(),
            RELEASE_CHANNEL_SCHEMA_VERSION
        ),
    }
}

pub fn save_release_channel_store_file(
    path: &Path,
    payload: &ReleaseChannelStoreFile,
) -> Result<()> {
    let mut encoded =
        serde_json::to_string_pretty(payload).context("failed to encode release channel store")?;
    encoded.push('\n');
    let parent = path.parent().ok_or_else(|| {
        anyhow!(
            "release channel path {} does not have a parent directory",
            path.display()
        )
    })?;
    std::fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create release channel directory {}",
            parent.display()
        )
    })?;
    write_text_atomic(path, &encoded)
}

pub fn save_release_channel_store(path: &Path, channel: ReleaseChannel) -> Result<()> {
    let existing = load_release_channel_store_file(path)?;
    let payload = ReleaseChannelStoreFile {
        schema_version: RELEASE_CHANNEL_SCHEMA_VERSION,
        release_channel: channel,
        rollback: existing.map(|store| store.rollback).unwrap_or_default(),
    };
    save_release_channel_store_file(path, &payload)
}

pub fn compare_versions(current: &str, latest: &str) -> Option<std::cmp::Ordering> {
    let current_segments = parse_version_segments(current)?;
    let latest_segments = parse_version_segments(latest)?;
    let max_len = current_segments.len().max(latest_segments.len());
    for index in 0..max_len {
        let current_value = *current_segments.get(index).unwrap_or(&0);
        let latest_value = *latest_segments.get(index).unwrap_or(&0);
        match current_value.cmp(&latest_value) {
            std::cmp::Ordering::Less => return Some(std::cmp::Ordering::Less),
            std::cmp::Ordering::Greater => return Some(std::cmp::Ordering::Greater),
            std::cmp::Ordering::Equal => {}
        }
    }
    Some(std::cmp::Ordering::Equal)
}

pub fn resolve_latest_channel_release(
    channel: ReleaseChannel,
    url: &str,
) -> Result<Option<String>> {
    let releases = fetch_release_records(url)?;
    Ok(select_latest_channel_release(channel, &releases))
}

pub fn resolve_latest_channel_release_cached(
    channel: ReleaseChannel,
    url: &str,
    cache_path: &Path,
    cache_ttl_ms: u64,
) -> Result<LatestChannelReleaseResolution> {
    let now_ms = current_unix_timestamp_ms();
    let mut stale_cache_releases: Option<Vec<GitHubReleaseRecord>> = None;

    if let Ok(Some(cache)) = cache::load_release_lookup_cache(cache_path, url) {
        let age_ms = now_ms.saturating_sub(cache.fetched_at_unix_ms);
        if age_ms <= cache_ttl_ms {
            return Ok(LatestChannelReleaseResolution {
                latest: select_latest_channel_release(channel, &cache.releases),
                source: ReleaseLookupSource::CacheFresh,
            });
        }
        stale_cache_releases = Some(cache.releases);
    }

    match fetch_release_records(url) {
        Ok(releases) => {
            let _ = cache::save_release_lookup_cache(cache_path, url, now_ms, &releases);
            Ok(LatestChannelReleaseResolution {
                latest: select_latest_channel_release(channel, &releases),
                source: ReleaseLookupSource::Live,
            })
        }
        Err(error) => {
            if let Some(releases) = stale_cache_releases {
                return Ok(LatestChannelReleaseResolution {
                    latest: select_latest_channel_release(channel, &releases),
                    source: ReleaseLookupSource::CacheStaleFallback,
                });
            }
            Err(error)
        }
    }
}

pub fn select_latest_channel_release(
    channel: ReleaseChannel,
    releases: &[GitHubReleaseRecord],
) -> Option<String> {
    let stable = releases
        .iter()
        .find(|item| !item.draft && !item.prerelease)
        .map(|item| item.tag_name.trim().to_string())
        .filter(|tag| !tag.is_empty());
    let prerelease = releases
        .iter()
        .find(|item| !item.draft && item.prerelease)
        .map(|item| item.tag_name.trim().to_string())
        .filter(|tag| !tag.is_empty());

    match channel {
        ReleaseChannel::Stable => stable,
        ReleaseChannel::Beta | ReleaseChannel::Dev => prerelease.or(stable),
    }
}

fn parse_version_segments(raw: &str) -> Option<Vec<u64>> {
    let trimmed = raw.trim().trim_start_matches('v').trim_start_matches('V');
    let core = trimmed
        .split_once('-')
        .map(|(left, _)| left)
        .unwrap_or(trimmed);
    if core.is_empty() {
        return None;
    }
    let mut segments = Vec::new();
    for token in core.split('.') {
        segments.push(token.parse::<u64>().ok()?);
    }
    if segments.is_empty() {
        return None;
    }
    Some(segments)
}

pub fn fetch_release_records(url: &str) -> Result<Vec<GitHubReleaseRecord>> {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| handle.block_on(fetch_release_records_async(url)))
    } else {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("failed to create runtime for release-channel check")?;
        runtime.block_on(fetch_release_records_async(url))
    }
}

fn current_unix_timestamp_ms() -> u64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => duration.as_millis().min(u64::MAX as u128) as u64,
        Err(_) => 0,
    }
}

async fn fetch_release_records_async(url: &str) -> Result<Vec<GitHubReleaseRecord>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(RELEASE_LOOKUP_TIMEOUT_MS))
        .build()
        .context("failed to construct HTTP client for release-channel check")?;
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, RELEASE_LOOKUP_USER_AGENT)
        .send()
        .await
        .with_context(|| format!("failed to fetch release metadata from '{}'", url))?;
    if !response.status().is_success() {
        bail!(
            "release lookup request to '{}' returned status {}",
            url,
            response.status()
        );
    }
    response
        .json::<Vec<GitHubReleaseRecord>>()
        .await
        .with_context(|| format!("failed to parse release lookup response from '{}'", url))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_compare_versions_handles_prefix_suffix_and_invalid_input() {
        assert_eq!(
            compare_versions("0.1.0", "0.1.1"),
            Some(std::cmp::Ordering::Less)
        );
        assert_eq!(
            compare_versions("v1.2.3-beta.1", "1.2.3"),
            Some(std::cmp::Ordering::Equal)
        );
        assert_eq!(
            compare_versions("1.2.3", "1.2.2"),
            Some(std::cmp::Ordering::Greater)
        );
        assert_eq!(compare_versions("abc", "1.0.0"), None);
    }
}
