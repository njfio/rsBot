use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::write_text_atomic;

use super::{GitHubReleaseRecord, RELEASE_LOOKUP_CACHE_SCHEMA_VERSION};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct ReleaseLookupCacheFile {
    pub(super) schema_version: u32,
    pub(super) source_url: String,
    pub(super) fetched_at_unix_ms: u64,
    pub(super) releases: Vec<GitHubReleaseRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ReleaseCacheAgeCounters {
    pub(super) freshness: &'static str,
    pub(super) next_refresh_in_ms: u64,
    pub(super) stale_by_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReleaseCachePruneDecision {
    KeepFresh,
    RemoveStale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReleaseCachePruneRecoveryReason {
    InvalidPayload,
    UnsupportedSchema,
}

impl ReleaseCachePruneRecoveryReason {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            ReleaseCachePruneRecoveryReason::InvalidPayload => "invalid_payload",
            ReleaseCachePruneRecoveryReason::UnsupportedSchema => "unsupported_schema",
        }
    }
}

pub(super) enum ReleaseCachePruneLoadOutcome {
    Missing,
    Present(ReleaseLookupCacheFile),
    RecoverableError {
        reason: ReleaseCachePruneRecoveryReason,
    },
    FatalError(anyhow::Error),
}

pub(super) fn compute_release_cache_age_counters(
    age_ms: u64,
    ttl_ms: u64,
) -> ReleaseCacheAgeCounters {
    if age_ms <= ttl_ms {
        return ReleaseCacheAgeCounters {
            freshness: "fresh",
            next_refresh_in_ms: ttl_ms.saturating_sub(age_ms),
            stale_by_ms: 0,
        };
    }
    ReleaseCacheAgeCounters {
        freshness: "stale",
        next_refresh_in_ms: 0,
        stale_by_ms: age_ms.saturating_sub(ttl_ms),
    }
}

pub(super) fn decide_release_cache_prune(age_ms: u64, ttl_ms: u64) -> ReleaseCachePruneDecision {
    if age_ms <= ttl_ms {
        return ReleaseCachePruneDecision::KeepFresh;
    }
    ReleaseCachePruneDecision::RemoveStale
}

pub(super) fn compute_release_cache_expires_at_unix_ms(
    fetched_at_unix_ms: u64,
    ttl_ms: u64,
) -> u64 {
    fetched_at_unix_ms.saturating_add(ttl_ms)
}

pub(super) fn is_release_cache_expired(age_ms: u64, ttl_ms: u64) -> bool {
    age_ms > ttl_ms
}

pub(super) fn load_release_lookup_cache_file(
    path: &Path,
) -> Result<Option<ReleaseLookupCacheFile>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read release lookup cache {}", path.display()))?;
    let parsed = serde_json::from_str::<ReleaseLookupCacheFile>(&raw)
        .with_context(|| format!("failed to parse release lookup cache {}", path.display()))?;
    if parsed.schema_version != RELEASE_LOOKUP_CACHE_SCHEMA_VERSION {
        bail!(
            "unsupported release lookup cache schema_version {} in {} (expected {})",
            parsed.schema_version,
            path.display(),
            RELEASE_LOOKUP_CACHE_SCHEMA_VERSION
        );
    }
    Ok(Some(parsed))
}

pub(super) fn load_release_lookup_cache_for_prune(path: &Path) -> ReleaseCachePruneLoadOutcome {
    if !path.exists() {
        return ReleaseCachePruneLoadOutcome::Missing;
    }

    let raw = match std::fs::read_to_string(path)
        .with_context(|| format!("failed to read release lookup cache {}", path.display()))
    {
        Ok(raw) => raw,
        Err(error) => return ReleaseCachePruneLoadOutcome::FatalError(error),
    };

    let parsed = match serde_json::from_str::<ReleaseLookupCacheFile>(&raw)
        .with_context(|| format!("failed to parse release lookup cache {}", path.display()))
    {
        Ok(parsed) => parsed,
        Err(_) => {
            return ReleaseCachePruneLoadOutcome::RecoverableError {
                reason: ReleaseCachePruneRecoveryReason::InvalidPayload,
            }
        }
    };

    if parsed.schema_version != RELEASE_LOOKUP_CACHE_SCHEMA_VERSION {
        return ReleaseCachePruneLoadOutcome::RecoverableError {
            reason: ReleaseCachePruneRecoveryReason::UnsupportedSchema,
        };
    }

    ReleaseCachePruneLoadOutcome::Present(parsed)
}

pub(super) fn load_release_lookup_cache(
    path: &Path,
    url: &str,
) -> Result<Option<ReleaseLookupCacheFile>> {
    let Some(parsed) = load_release_lookup_cache_file(path)? else {
        return Ok(None);
    };
    if parsed.source_url != url {
        return Ok(None);
    }
    Ok(Some(parsed))
}

pub(super) fn save_release_lookup_cache(
    path: &Path,
    url: &str,
    fetched_at_unix_ms: u64,
    releases: &[GitHubReleaseRecord],
) -> Result<()> {
    let payload = ReleaseLookupCacheFile {
        schema_version: RELEASE_LOOKUP_CACHE_SCHEMA_VERSION,
        source_url: url.to_string(),
        fetched_at_unix_ms,
        releases: releases.to_vec(),
    };
    let mut encoded = serde_json::to_string_pretty(&payload)
        .context("failed to encode release lookup cache payload")?;
    encoded.push('\n');
    let parent = path.parent().ok_or_else(|| {
        anyhow!(
            "release lookup cache path {} does not have a parent directory",
            path.display()
        )
    })?;
    std::fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create release lookup cache directory {}",
            parent.display()
        )
    })?;
    write_text_atomic(path, &encoded)
}
