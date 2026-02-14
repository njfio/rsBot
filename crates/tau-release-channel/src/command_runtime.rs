use anyhow::{anyhow, bail, Result};
use std::path::{Path, PathBuf};

mod update_state;

#[cfg(test)]
use crate::cache::load_release_lookup_cache;
use crate::cache::{
    compute_release_cache_age_counters, compute_release_cache_expires_at_unix_ms,
    decide_release_cache_prune, is_release_cache_expired, load_release_lookup_cache_file,
    load_release_lookup_cache_for_prune, save_release_lookup_cache, ReleaseCachePruneDecision,
    ReleaseCachePruneLoadOutcome,
};
#[cfg(test)]
use crate::cache::{ReleaseCacheAgeCounters, ReleaseCachePruneRecoveryReason};
use crate::{
    compare_versions, fetch_release_records, release_lookup_url,
    resolve_latest_channel_release_cached, select_latest_channel_release, GitHubReleaseRecord,
    LatestChannelReleaseResolution, ReleaseLookupSource, RELEASE_LOOKUP_CACHE_TTL_MS,
};
#[cfg(test)]
use crate::{
    resolve_latest_channel_release, RELEASE_LOOKUP_CACHE_SCHEMA_VERSION, RELEASE_LOOKUP_USER_AGENT,
};
use update_state::{
    load_release_update_state_file, save_release_update_state_file, ReleaseUpdateStateFile,
};

pub const RELEASE_CHANNEL_USAGE: &str =
    "usage: /release-channel [show|set <stable|beta|dev>|check|plan [--target <version>] [--dry-run]|apply [--target <version>] [--dry-run]|cache <show|clear|refresh|prune>]";
pub(crate) const RELEASE_UPDATE_STATE_SCHEMA_VERSION: u32 = 1;
pub(crate) use crate::ReleaseChannel;
#[cfg(test)]
pub(crate) use crate::{load_release_channel_store, save_release_channel_store};
pub(crate) use crate::{
    load_release_channel_store_file, save_release_channel_store_file,
    ReleaseChannelRollbackMetadata, ReleaseChannelStoreFile, RELEASE_CHANNEL_SCHEMA_VERSION,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReleaseChannelCommand {
    Show,
    Set(ReleaseChannel),
    Check,
    Plan {
        target: Option<String>,
        dry_run: bool,
    },
    Apply {
        target: Option<String>,
        dry_run: bool,
    },
    CacheShow,
    CacheClear,
    CacheRefresh,
    CachePrune,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReleaseUpdateAction {
    Upgrade,
    Noop,
    Blocked,
}

impl ReleaseUpdateAction {
    fn as_str(self) -> &'static str {
        match self {
            ReleaseUpdateAction::Upgrade => "upgrade",
            ReleaseUpdateAction::Noop => "noop",
            ReleaseUpdateAction::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReleaseUpdateGuardCode {
    Ok,
    InvalidCurrentVersion,
    InvalidTargetVersion,
    StablePrereleaseDisallowed,
    MajorVersionJumpBlocked,
}

impl ReleaseUpdateGuardCode {
    fn as_str(self) -> &'static str {
        match self {
            ReleaseUpdateGuardCode::Ok => "ok",
            ReleaseUpdateGuardCode::InvalidCurrentVersion => "invalid_current_version",
            ReleaseUpdateGuardCode::InvalidTargetVersion => "invalid_target_version",
            ReleaseUpdateGuardCode::StablePrereleaseDisallowed => "stable_prerelease_disallowed",
            ReleaseUpdateGuardCode::MajorVersionJumpBlocked => "major_version_jump_blocked",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReleaseUpdateGuardOutcome {
    code: ReleaseUpdateGuardCode,
    reason: String,
}

impl ReleaseUpdateGuardOutcome {
    fn ok() -> Self {
        Self {
            code: ReleaseUpdateGuardCode::Ok,
            reason: "target_compatible".to_string(),
        }
    }

    fn blocked(code: ReleaseUpdateGuardCode, reason: impl Into<String>) -> Self {
        Self {
            code,
            reason: reason.into(),
        }
    }

    fn allowed(&self) -> bool {
        self.code == ReleaseUpdateGuardCode::Ok
    }
}

pub(crate) fn parse_release_channel_command(command_args: &str) -> Result<ReleaseChannelCommand> {
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return Ok(ReleaseChannelCommand::Show);
    }

    if tokens.len() == 1 && tokens[0] == "show" {
        return Ok(ReleaseChannelCommand::Show);
    }

    if tokens.len() == 1 && tokens[0] == "check" {
        return Ok(ReleaseChannelCommand::Check);
    }

    if tokens.first() == Some(&"plan") {
        let options = parse_release_update_command_options(&tokens[1..])?;
        return Ok(ReleaseChannelCommand::Plan {
            target: options.target,
            dry_run: options.dry_run,
        });
    }

    if tokens.first() == Some(&"apply") {
        let options = parse_release_update_command_options(&tokens[1..])?;
        return Ok(ReleaseChannelCommand::Apply {
            target: options.target,
            dry_run: options.dry_run,
        });
    }

    if tokens.len() == 2 && tokens[0] == "set" {
        let channel = tokens[1].parse::<ReleaseChannel>()?;
        return Ok(ReleaseChannelCommand::Set(channel));
    }

    if tokens.len() == 2 && tokens[0] == "cache" {
        return match tokens[1] {
            "show" => Ok(ReleaseChannelCommand::CacheShow),
            "clear" => Ok(ReleaseChannelCommand::CacheClear),
            "refresh" => Ok(ReleaseChannelCommand::CacheRefresh),
            "prune" => Ok(ReleaseChannelCommand::CachePrune),
            _ => bail!("{RELEASE_CHANNEL_USAGE}"),
        };
    }

    bail!("{RELEASE_CHANNEL_USAGE}");
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ReleaseUpdateCommandOptions {
    target: Option<String>,
    dry_run: bool,
}

fn parse_release_update_command_options(tokens: &[&str]) -> Result<ReleaseUpdateCommandOptions> {
    let mut options = ReleaseUpdateCommandOptions::default();
    let mut index = 0usize;
    while index < tokens.len() {
        match tokens[index] {
            "--dry-run" => {
                options.dry_run = true;
                index += 1;
            }
            "--target" => {
                let Some(raw_target) = tokens.get(index + 1) else {
                    bail!("{RELEASE_CHANNEL_USAGE}");
                };
                let target = raw_target.trim();
                if target.is_empty() {
                    bail!("{RELEASE_CHANNEL_USAGE}");
                }
                options.target = Some(target.to_string());
                index += 2;
            }
            _ => bail!("{RELEASE_CHANNEL_USAGE}"),
        }
    }
    Ok(options)
}

fn release_lookup_cache_path_for_release_channel_store(path: &Path) -> Result<PathBuf> {
    let parent = path.parent().ok_or_else(|| {
        anyhow!(
            "release channel path {} does not have a parent directory",
            path.display()
        )
    })?;
    Ok(parent.join("release-lookup-cache.json"))
}

fn release_update_state_path_for_release_channel_store(path: &Path) -> Result<PathBuf> {
    let parent = path.parent().ok_or_else(|| {
        anyhow!(
            "release channel path {} does not have a parent directory",
            path.display()
        )
    })?;
    Ok(parent.join("release-update-state.json"))
}

fn resolve_release_channel_and_metadata(
    path: &Path,
) -> Result<(ReleaseChannel, &'static str, ReleaseChannelRollbackMetadata)> {
    match load_release_channel_store_file(path)? {
        Some(store) => Ok((store.release_channel, "store", store.rollback)),
        None => Ok((
            ReleaseChannel::Stable,
            "default",
            ReleaseChannelRollbackMetadata::default(),
        )),
    }
}

fn render_rollback_fields(rollback: &ReleaseChannelRollbackMetadata) -> String {
    format!(
        "rollback_channel={} rollback_version={} rollback_reason={} rollback_reference_unix_ms={}",
        rollback
            .previous_channel
            .map(|value| value.as_str().to_string())
            .unwrap_or_else(|| "none".to_string()),
        rollback
            .previous_version
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("none"),
        rollback
            .reason
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("none"),
        rollback
            .reference_unix_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
    )
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

fn major_version(raw: &str) -> Option<u64> {
    parse_version_segments(raw).and_then(|segments| segments.first().copied())
}

fn evaluate_release_update_guard(
    channel: ReleaseChannel,
    current_version: &str,
    target_version: &str,
) -> ReleaseUpdateGuardOutcome {
    if parse_version_segments(current_version).is_none() {
        return ReleaseUpdateGuardOutcome::blocked(
            ReleaseUpdateGuardCode::InvalidCurrentVersion,
            format!("invalid current version '{}'", current_version),
        );
    }
    if parse_version_segments(target_version).is_none() {
        return ReleaseUpdateGuardOutcome::blocked(
            ReleaseUpdateGuardCode::InvalidTargetVersion,
            format!("invalid target version '{}'", target_version),
        );
    }
    if channel == ReleaseChannel::Stable && target_version.contains('-') {
        return ReleaseUpdateGuardOutcome::blocked(
            ReleaseUpdateGuardCode::StablePrereleaseDisallowed,
            format!(
                "stable channel blocks prerelease target '{}'",
                target_version
            ),
        );
    }
    let Some(current_major) = major_version(current_version) else {
        return ReleaseUpdateGuardOutcome::blocked(
            ReleaseUpdateGuardCode::InvalidCurrentVersion,
            format!("invalid current version '{}'", current_version),
        );
    };
    let Some(target_major) = major_version(target_version) else {
        return ReleaseUpdateGuardOutcome::blocked(
            ReleaseUpdateGuardCode::InvalidTargetVersion,
            format!("invalid target version '{}'", target_version),
        );
    };
    if target_major > current_major.saturating_add(1) {
        return ReleaseUpdateGuardOutcome::blocked(
            ReleaseUpdateGuardCode::MajorVersionJumpBlocked,
            format!(
                "major version jump blocked current_major={} target_major={}",
                current_major, target_major
            ),
        );
    }
    ReleaseUpdateGuardOutcome::ok()
}

fn resolve_release_update_action(
    guard: &ReleaseUpdateGuardOutcome,
    current_version: &str,
    target_version: &str,
) -> ReleaseUpdateAction {
    if !guard.allowed() {
        return ReleaseUpdateAction::Blocked;
    }
    match compare_versions(current_version, target_version) {
        Some(std::cmp::Ordering::Less) => ReleaseUpdateAction::Upgrade,
        Some(std::cmp::Ordering::Equal | std::cmp::Ordering::Greater) => ReleaseUpdateAction::Noop,
        None => ReleaseUpdateAction::Blocked,
    }
}

fn resolve_channel_latest_or_unknown(
    channel: ReleaseChannel,
    releases: &[GitHubReleaseRecord],
) -> String {
    select_latest_channel_release(channel, releases).unwrap_or_else(|| "unknown".to_string())
}

fn current_unix_timestamp_ms() -> u64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => duration.as_millis().min(u64::MAX as u128) as u64,
        Err(_) => 0,
    }
}

fn release_lookup_source_label(source: ReleaseLookupSource) -> &'static str {
    match source {
        ReleaseLookupSource::Live => "live",
        ReleaseLookupSource::CacheFresh => "cache_fresh",
        ReleaseLookupSource::CacheStaleFallback => "cache_stale_fallback",
    }
}

fn render_release_channel_check_with_lookup<F>(
    path: &Path,
    current_version: &str,
    lookup: F,
) -> String
where
    F: Fn(ReleaseChannel) -> Result<LatestChannelReleaseResolution>,
{
    let (channel, channel_source, _) = match resolve_release_channel_and_metadata(path) {
        Ok(resolution) => resolution,
        Err(error) => {
            return format!(
                "release channel error: path={} error={error}",
                path.display()
            );
        }
    };

    match lookup(channel) {
        Ok(resolution) => {
            let lookup_source = release_lookup_source_label(resolution.source);
            let Some(latest) = resolution.latest else {
                return format!(
                    "release channel check: path={} channel={} channel_source={} current={} latest=unknown status=unknown source=github lookup_source={} error=no_release_records",
                    path.display(),
                    channel,
                    channel_source,
                    current_version,
                    lookup_source
                );
            };
            let status = match compare_versions(current_version, &latest) {
                Some(std::cmp::Ordering::Less) => "update_available",
                Some(std::cmp::Ordering::Equal | std::cmp::Ordering::Greater) => "up_to_date",
                None => "unknown",
            };
            format!(
                "release channel check: path={} channel={} channel_source={} current={} latest={} status={} source=github lookup_source={}",
                path.display(),
                channel,
                channel_source,
                current_version,
                latest,
                status,
                lookup_source
            )
        }
        Err(error) => format!(
            "release channel check: path={} channel={} channel_source={} current={} latest=unknown status=unknown source=github lookup_source=unknown error={error}",
            path.display(),
            channel,
            channel_source,
            current_version
        ),
    }
}

fn execute_release_channel_check_with_lookup_options(
    path: &Path,
    current_version: &str,
    lookup_url: &str,
    cache_path: &Path,
    cache_ttl_ms: u64,
) -> String {
    render_release_channel_check_with_lookup(path, current_version, |channel| {
        resolve_latest_channel_release_cached(channel, lookup_url, cache_path, cache_ttl_ms)
    })
}

fn execute_release_channel_cache_refresh_with_lookup_options(
    cache_path: &Path,
    lookup_url: &str,
) -> String {
    let fetched_at_unix_ms = current_unix_timestamp_ms();
    match fetch_release_records(lookup_url) {
        Ok(releases) => match save_release_lookup_cache(
            cache_path,
            lookup_url,
            fetched_at_unix_ms,
            &releases,
        ) {
            Ok(()) => format!(
                "release cache refresh: path={} status=refreshed entries={} fetched_at_unix_ms={} source_url={}",
                cache_path.display(),
                releases.len(),
                fetched_at_unix_ms,
                lookup_url
            ),
            Err(error) => format!(
                "release channel error: path={} error={error}",
                cache_path.display()
            ),
        },
        Err(error) => format!(
            "release channel error: path={} error={error}",
            cache_path.display()
        ),
    }
}

fn execute_release_channel_cache_prune_with_options(
    cache_path: &Path,
    cache_ttl_ms: u64,
) -> String {
    match load_release_lookup_cache_for_prune(cache_path) {
        ReleaseCachePruneLoadOutcome::Missing => format!(
            "release cache prune: path={} status=missing",
            cache_path.display()
        ),
        ReleaseCachePruneLoadOutcome::Present(cache) => {
            let age_ms = current_unix_timestamp_ms().saturating_sub(cache.fetched_at_unix_ms);
            let age_counters = compute_release_cache_age_counters(age_ms, cache_ttl_ms);
            let expires_at_unix_ms =
                compute_release_cache_expires_at_unix_ms(cache.fetched_at_unix_ms, cache_ttl_ms);
            let is_expired = is_release_cache_expired(age_ms, cache_ttl_ms);
            match decide_release_cache_prune(age_ms, cache_ttl_ms) {
                ReleaseCachePruneDecision::KeepFresh => format!(
                    "release cache prune: path={} status=kept reason=fresh entries={} fetched_at_unix_ms={} age_ms={} ttl_ms={} freshness={} next_refresh_in_ms={} stale_by_ms={} expires_at_unix_ms={} is_expired={}",
                    cache_path.display(),
                    cache.releases.len(),
                    cache.fetched_at_unix_ms,
                    age_ms,
                    cache_ttl_ms,
                    age_counters.freshness,
                    age_counters.next_refresh_in_ms,
                    age_counters.stale_by_ms,
                    expires_at_unix_ms,
                    is_expired
                ),
                ReleaseCachePruneDecision::RemoveStale => match std::fs::remove_file(cache_path) {
                    Ok(()) => format!(
                        "release cache prune: path={} status=removed reason=stale entries={} fetched_at_unix_ms={} age_ms={} ttl_ms={} freshness={} next_refresh_in_ms={} stale_by_ms={} expires_at_unix_ms={} is_expired={}",
                        cache_path.display(),
                        cache.releases.len(),
                        cache.fetched_at_unix_ms,
                        age_ms,
                        cache_ttl_ms,
                        age_counters.freshness,
                        age_counters.next_refresh_in_ms,
                        age_counters.stale_by_ms,
                        expires_at_unix_ms,
                        is_expired
                    ),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => format!(
                        "release cache prune: path={} status=missing",
                        cache_path.display()
                    ),
                    Err(error) => format!(
                        "release channel error: path={} error={error}",
                        cache_path.display()
                    ),
                },
            }
        }
        ReleaseCachePruneLoadOutcome::RecoverableError { reason } => {
            match std::fs::remove_file(cache_path) {
                Ok(()) => format!(
                    "release cache prune: path={} status=removed reason={} recovery_action=removed_invalid_cache",
                    cache_path.display(),
                    reason.as_str()
                ),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => format!(
                    "release cache prune: path={} status=missing reason={} recovery_action=already_missing",
                    cache_path.display(),
                    reason.as_str()
                ),
                Err(error) => format!(
                    "release channel error: path={} error={error}",
                    cache_path.display()
                ),
            }
        }
        ReleaseCachePruneLoadOutcome::FatalError(error) => format!(
            "release channel error: path={} error={error}",
            cache_path.display()
        ),
    }
}

fn sanitize_output_token(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "none".to_string();
    }
    trimmed.split_whitespace().collect::<Vec<_>>().join("_")
}

fn resolve_release_update_target(
    channel: ReleaseChannel,
    lookup_url: &str,
    cache_path: &Path,
    cache_ttl_ms: u64,
    target_override: Option<&str>,
) -> Result<(String, String, String)> {
    if let Some(target) = target_override {
        let normalized = target.trim().to_string();
        return Ok((normalized.clone(), normalized, "override".to_string()));
    }
    let resolution =
        resolve_latest_channel_release_cached(channel, lookup_url, cache_path, cache_ttl_ms)?;
    let lookup_source = release_lookup_source_label(resolution.source).to_string();
    let Some(latest) = resolution.latest else {
        bail!("no release records for channel {}", channel);
    };
    Ok((latest.clone(), latest, lookup_source))
}

fn execute_release_channel_plan_with_lookup_options(
    path: &Path,
    lookup_url: &str,
    cache_ttl_ms: u64,
    dry_run: bool,
    target_override: Option<&str>,
) -> String {
    let (channel, channel_source, rollback) = match resolve_release_channel_and_metadata(path) {
        Ok(resolution) => resolution,
        Err(error) => {
            return format!(
                "release channel error: path={} error={error}",
                path.display()
            );
        }
    };
    let cache_path = match release_lookup_cache_path_for_release_channel_store(path) {
        Ok(path) => path,
        Err(error) => {
            return format!(
                "release channel error: path={} error={error}",
                path.display()
            );
        }
    };
    let state_path = match release_update_state_path_for_release_channel_store(path) {
        Ok(path) => path,
        Err(error) => {
            return format!(
                "release channel error: path={} error={error}",
                path.display()
            );
        }
    };
    let existing_state = match load_release_update_state_file(&state_path) {
        Ok(state) => state,
        Err(error) => {
            return format!(
                "release channel error: path={} error={error}",
                state_path.display()
            );
        }
    };
    let (target_version, latest_version, lookup_source) = match resolve_release_update_target(
        channel,
        lookup_url,
        &cache_path,
        cache_ttl_ms,
        target_override,
    ) {
        Ok(resolution) => resolution,
        Err(error) => {
            return format!(
                "release channel plan: path={} channel={} channel_source={} current={} latest=unknown target=unknown action=blocked dry_run={} guard_code=no_release_records guard_reason={} lookup_source=unknown {}",
                path.display(),
                channel,
                channel_source,
                env!("CARGO_PKG_VERSION"),
                dry_run,
                sanitize_output_token(error.to_string().as_str()),
                render_rollback_fields(&rollback),
            );
        }
    };
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let guard = evaluate_release_update_guard(channel, &current_version, &target_version);
    let action = resolve_release_update_action(&guard, &current_version, &target_version);
    let planned_at_unix_ms = current_unix_timestamp_ms();
    let state = ReleaseUpdateStateFile {
        schema_version: RELEASE_UPDATE_STATE_SCHEMA_VERSION,
        channel,
        current_version: current_version.clone(),
        target_version: target_version.clone(),
        action: action.as_str().to_string(),
        dry_run,
        lookup_source: lookup_source.clone(),
        guard_code: guard.code.as_str().to_string(),
        guard_reason: guard.reason.clone(),
        planned_at_unix_ms,
        apply_attempts: existing_state
            .as_ref()
            .map(|state| state.apply_attempts)
            .unwrap_or(0),
        last_apply_unix_ms: existing_state
            .as_ref()
            .and_then(|state| state.last_apply_unix_ms),
        last_apply_status: existing_state
            .as_ref()
            .and_then(|state| state.last_apply_status.clone()),
        last_apply_target: existing_state
            .as_ref()
            .and_then(|state| state.last_apply_target.clone()),
        rollback_channel: rollback.previous_channel,
        rollback_version: rollback.previous_version.clone(),
    };
    if let Err(error) = save_release_update_state_file(&state_path, &state) {
        return format!(
            "release channel error: path={} error={error}",
            state_path.display()
        );
    }
    format!(
        "release channel plan: path={} state_path={} channel={} channel_source={} current={} latest={} target={} action={} dry_run={} lookup_source={} guard_code={} guard_reason={} planned_at_unix_ms={} {} status=saved",
        path.display(),
        state_path.display(),
        channel,
        channel_source,
        current_version,
        latest_version,
        target_version,
        action.as_str(),
        dry_run,
        lookup_source,
        guard.code.as_str(),
        sanitize_output_token(guard.reason.as_str()),
        planned_at_unix_ms,
        render_rollback_fields(&rollback),
    )
}

fn execute_release_channel_apply_with_lookup_options(
    path: &Path,
    lookup_url: &str,
    cache_ttl_ms: u64,
    dry_run: bool,
    target_override: Option<&str>,
) -> String {
    let (channel, channel_source, mut rollback) = match resolve_release_channel_and_metadata(path) {
        Ok(resolution) => resolution,
        Err(error) => {
            return format!(
                "release channel error: path={} error={error}",
                path.display()
            );
        }
    };
    let cache_path = match release_lookup_cache_path_for_release_channel_store(path) {
        Ok(path) => path,
        Err(error) => {
            return format!(
                "release channel error: path={} error={error}",
                path.display()
            );
        }
    };
    let state_path = match release_update_state_path_for_release_channel_store(path) {
        Ok(path) => path,
        Err(error) => {
            return format!(
                "release channel error: path={} error={error}",
                path.display()
            );
        }
    };
    let existing_state = match load_release_update_state_file(&state_path) {
        Ok(state) => state,
        Err(error) => {
            return format!(
                "release channel error: path={} error={error}",
                state_path.display()
            );
        }
    };

    let target_source_override = target_override
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let resolved = if let Some(target) = target_source_override {
        Ok((
            target.to_string(),
            target.to_string(),
            "override".to_string(),
        ))
    } else if let Some(state) = existing_state.as_ref() {
        Ok((
            state.target_version.clone(),
            state.target_version.clone(),
            "state".to_string(),
        ))
    } else {
        resolve_release_update_target(channel, lookup_url, &cache_path, cache_ttl_ms, None)
    };
    let (target_version, latest_version, lookup_source) = match resolved {
        Ok(values) => values,
        Err(error) => {
            return format!(
                "release channel apply: path={} channel={} channel_source={} current={} latest=unknown target=unknown action=blocked dry_run={} guard_code=no_release_records guard_reason={} lookup_source=unknown {} status=blocked",
                path.display(),
                channel,
                channel_source,
                env!("CARGO_PKG_VERSION"),
                dry_run,
                sanitize_output_token(error.to_string().as_str()),
                render_rollback_fields(&rollback),
            );
        }
    };
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let guard = evaluate_release_update_guard(channel, &current_version, &target_version);
    let action = resolve_release_update_action(&guard, &current_version, &target_version);
    let status = match action {
        ReleaseUpdateAction::Blocked => "blocked",
        ReleaseUpdateAction::Noop => "noop",
        ReleaseUpdateAction::Upgrade if dry_run => "dry_run",
        ReleaseUpdateAction::Upgrade => "applied_metadata",
    };

    let apply_timestamp = current_unix_timestamp_ms();
    if action == ReleaseUpdateAction::Upgrade && !dry_run {
        rollback.previous_channel = Some(channel);
        rollback.previous_version = Some(current_version.clone());
        rollback.reference_unix_ms = Some(apply_timestamp);
        rollback.reason = Some("apply_upgrade".to_string());
        let store_payload = ReleaseChannelStoreFile {
            schema_version: RELEASE_CHANNEL_SCHEMA_VERSION,
            release_channel: channel,
            rollback: rollback.clone(),
        };
        if let Err(error) = save_release_channel_store_file(path, &store_payload) {
            return format!(
                "release channel error: path={} error={error}",
                path.display()
            );
        }
    }

    let state = ReleaseUpdateStateFile {
        schema_version: RELEASE_UPDATE_STATE_SCHEMA_VERSION,
        channel,
        current_version: current_version.clone(),
        target_version: target_version.clone(),
        action: action.as_str().to_string(),
        dry_run,
        lookup_source: lookup_source.clone(),
        guard_code: guard.code.as_str().to_string(),
        guard_reason: guard.reason.clone(),
        planned_at_unix_ms: existing_state
            .as_ref()
            .map(|state| state.planned_at_unix_ms)
            .unwrap_or(apply_timestamp),
        apply_attempts: existing_state
            .as_ref()
            .map(|state| state.apply_attempts.saturating_add(1))
            .unwrap_or(1),
        last_apply_unix_ms: Some(apply_timestamp),
        last_apply_status: Some(status.to_string()),
        last_apply_target: Some(target_version.clone()),
        rollback_channel: rollback.previous_channel,
        rollback_version: rollback.previous_version.clone(),
    };
    if let Err(error) = save_release_update_state_file(&state_path, &state) {
        return format!(
            "release channel error: path={} error={error}",
            state_path.display()
        );
    }
    format!(
        "release channel apply: path={} state_path={} channel={} channel_source={} current={} latest={} target={} action={} dry_run={} lookup_source={} guard_code={} guard_reason={} apply_attempts={} last_apply_unix_ms={} {} status={}",
        path.display(),
        state_path.display(),
        channel,
        channel_source,
        current_version,
        latest_version,
        target_version,
        action.as_str(),
        dry_run,
        lookup_source,
        guard.code.as_str(),
        sanitize_output_token(guard.reason.as_str()),
        state.apply_attempts,
        apply_timestamp,
        render_rollback_fields(&rollback),
        status,
    )
}

pub fn execute_release_channel_command(command_args: &str, path: &Path) -> String {
    execute_release_channel_command_with_lookup_options(
        command_args,
        path,
        release_lookup_url(),
        RELEASE_LOOKUP_CACHE_TTL_MS,
    )
}

fn execute_release_channel_command_with_lookup_options(
    command_args: &str,
    path: &Path,
    lookup_url: &str,
    cache_ttl_ms: u64,
) -> String {
    let command = match parse_release_channel_command(command_args) {
        Ok(command) => command,
        Err(error) => {
            return format!(
                "release channel error: path={} error={error}",
                path.display()
            );
        }
    };

    match command {
        ReleaseChannelCommand::Show => match resolve_release_channel_and_metadata(path) {
            Ok((channel, source, rollback)) => format!(
                "release channel: path={} channel={} source={} {}",
                path.display(),
                channel,
                source,
                render_rollback_fields(&rollback),
            ),
            Err(error) => format!(
                "release channel error: path={} error={error}",
                path.display()
            ),
        },
        ReleaseChannelCommand::Set(channel) => {
            let existing = match load_release_channel_store_file(path) {
                Ok(state) => state,
                Err(error) => {
                    return format!(
                        "release channel error: path={} error={error}",
                        path.display()
                    );
                }
            };
            let previous_channel = existing
                .as_ref()
                .map(|store| store.release_channel)
                .unwrap_or(ReleaseChannel::Stable);
            let mut rollback = existing.map(|store| store.rollback).unwrap_or_default();
            if previous_channel != channel {
                rollback.previous_channel = Some(previous_channel);
                rollback.previous_version = Some(env!("CARGO_PKG_VERSION").to_string());
                rollback.reference_unix_ms = Some(current_unix_timestamp_ms());
                rollback.reason = Some("channel_switch".to_string());
            }
            let payload = ReleaseChannelStoreFile {
                schema_version: RELEASE_CHANNEL_SCHEMA_VERSION,
                release_channel: channel,
                rollback: rollback.clone(),
            };
            match save_release_channel_store_file(path, &payload) {
                Ok(()) => format!(
                    "release channel set: path={} channel={} previous_channel={} status=saved {}",
                    path.display(),
                    channel,
                    previous_channel,
                    render_rollback_fields(&rollback),
                ),
                Err(error) => format!(
                    "release channel error: path={} error={error}",
                    path.display()
                ),
            }
        }
        ReleaseChannelCommand::Check => {
            let cache_path = match release_lookup_cache_path_for_release_channel_store(path) {
                Ok(path) => path,
                Err(error) => {
                    return format!(
                        "release channel error: path={} error={error}",
                        path.display()
                    );
                }
            };
            execute_release_channel_check_with_lookup_options(
                path,
                env!("CARGO_PKG_VERSION"),
                lookup_url,
                &cache_path,
                cache_ttl_ms,
            )
        }
        ReleaseChannelCommand::Plan { target, dry_run } => {
            execute_release_channel_plan_with_lookup_options(
                path,
                lookup_url,
                cache_ttl_ms,
                dry_run,
                target.as_deref(),
            )
        }
        ReleaseChannelCommand::Apply { target, dry_run } => {
            execute_release_channel_apply_with_lookup_options(
                path,
                lookup_url,
                cache_ttl_ms,
                dry_run,
                target.as_deref(),
            )
        }
        ReleaseChannelCommand::CacheRefresh => {
            let cache_path = match release_lookup_cache_path_for_release_channel_store(path) {
                Ok(path) => path,
                Err(error) => {
                    return format!(
                        "release channel error: path={} error={error}",
                        path.display()
                    );
                }
            };
            execute_release_channel_cache_refresh_with_lookup_options(&cache_path, lookup_url)
        }
        ReleaseChannelCommand::CachePrune => {
            let cache_path = match release_lookup_cache_path_for_release_channel_store(path) {
                Ok(path) => path,
                Err(error) => {
                    return format!(
                        "release channel error: path={} error={error}",
                        path.display()
                    );
                }
            };
            execute_release_channel_cache_prune_with_options(&cache_path, cache_ttl_ms)
        }
        ReleaseChannelCommand::CacheShow => {
            let cache_path = match release_lookup_cache_path_for_release_channel_store(path) {
                Ok(path) => path,
                Err(error) => {
                    return format!(
                        "release channel error: path={} error={error}",
                        path.display()
                    );
                }
            };
            match load_release_lookup_cache_file(&cache_path) {
                Ok(Some(cache)) => {
                    let age_ms =
                        current_unix_timestamp_ms().saturating_sub(cache.fetched_at_unix_ms);
                    let age_counters = compute_release_cache_age_counters(age_ms, cache_ttl_ms);
                    let expires_at_unix_ms = compute_release_cache_expires_at_unix_ms(
                        cache.fetched_at_unix_ms,
                        cache_ttl_ms,
                    );
                    let is_expired = is_release_cache_expired(age_ms, cache_ttl_ms);
                    let stable_latest =
                        resolve_channel_latest_or_unknown(ReleaseChannel::Stable, &cache.releases);
                    let beta_latest =
                        resolve_channel_latest_or_unknown(ReleaseChannel::Beta, &cache.releases);
                    let dev_latest =
                        resolve_channel_latest_or_unknown(ReleaseChannel::Dev, &cache.releases);
                    format!(
                        "release cache: path={} status=present schema_version={} entries={} fetched_at_unix_ms={} age_ms={} ttl_ms={} freshness={} next_refresh_in_ms={} stale_by_ms={} expires_at_unix_ms={} is_expired={} source_url={} stable_latest={} beta_latest={} dev_latest={}",
                        cache_path.display(),
                        cache.schema_version,
                        cache.releases.len(),
                        cache.fetched_at_unix_ms,
                        age_ms,
                        cache_ttl_ms,
                        age_counters.freshness,
                        age_counters.next_refresh_in_ms,
                        age_counters.stale_by_ms,
                        expires_at_unix_ms,
                        is_expired,
                        cache.source_url,
                        stable_latest,
                        beta_latest,
                        dev_latest
                    )
                }
                Ok(None) => format!(
                    "release cache: path={} status=missing",
                    cache_path.display()
                ),
                Err(error) => format!(
                    "release channel error: path={} error={error}",
                    cache_path.display()
                ),
            }
        }
        ReleaseChannelCommand::CacheClear => {
            let cache_path = match release_lookup_cache_path_for_release_channel_store(path) {
                Ok(path) => path,
                Err(error) => {
                    return format!(
                        "release channel error: path={} error={error}",
                        path.display()
                    );
                }
            };
            match std::fs::remove_file(&cache_path) {
                Ok(()) => format!(
                    "release cache clear: path={} status=removed",
                    cache_path.display()
                ),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => format!(
                    "release cache clear: path={} status=already_missing",
                    cache_path.display()
                ),
                Err(error) => format!(
                    "release channel error: path={} error={error}",
                    cache_path.display()
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    #[test]
    fn unit_parse_release_channel_command_supports_show_set_check_plan_apply_and_cache_subcommands()
    {
        assert_eq!(
            parse_release_channel_command("").expect("default command"),
            ReleaseChannelCommand::Show
        );
        assert_eq!(
            parse_release_channel_command("show").expect("show command"),
            ReleaseChannelCommand::Show
        );
        assert_eq!(
            parse_release_channel_command("set beta").expect("set command"),
            ReleaseChannelCommand::Set(ReleaseChannel::Beta)
        );
        assert_eq!(
            parse_release_channel_command("check").expect("check command"),
            ReleaseChannelCommand::Check
        );
        assert_eq!(
            parse_release_channel_command("plan").expect("plan command"),
            ReleaseChannelCommand::Plan {
                target: None,
                dry_run: false,
            }
        );
        assert_eq!(
            parse_release_channel_command("plan --target v1.2.3 --dry-run")
                .expect("plan with options"),
            ReleaseChannelCommand::Plan {
                target: Some("v1.2.3".to_string()),
                dry_run: true,
            }
        );
        assert_eq!(
            parse_release_channel_command("apply --dry-run").expect("apply command"),
            ReleaseChannelCommand::Apply {
                target: None,
                dry_run: true,
            }
        );
        assert_eq!(
            parse_release_channel_command("apply --target v2.0.0").expect("apply target command"),
            ReleaseChannelCommand::Apply {
                target: Some("v2.0.0".to_string()),
                dry_run: false,
            }
        );
        assert_eq!(
            parse_release_channel_command("cache show").expect("cache show command"),
            ReleaseChannelCommand::CacheShow
        );
        assert_eq!(
            parse_release_channel_command("cache clear").expect("cache clear command"),
            ReleaseChannelCommand::CacheClear
        );
        assert_eq!(
            parse_release_channel_command("cache refresh").expect("cache refresh command"),
            ReleaseChannelCommand::CacheRefresh
        );
        assert_eq!(
            parse_release_channel_command("cache prune").expect("cache prune command"),
            ReleaseChannelCommand::CachePrune
        );

        let invalid = parse_release_channel_command("set nightly").expect_err("invalid channel");
        assert!(invalid.to_string().contains("expected stable|beta|dev"));

        let invalid = parse_release_channel_command("check now").expect_err("invalid extra arg");
        assert!(invalid.to_string().contains(RELEASE_CHANNEL_USAGE));
        let invalid =
            parse_release_channel_command("cache inspect").expect_err("invalid cache subcommand");
        assert!(invalid.to_string().contains(RELEASE_CHANNEL_USAGE));
        let invalid = parse_release_channel_command("plan --target")
            .expect_err("plan missing target value should fail");
        assert!(invalid.to_string().contains(RELEASE_CHANNEL_USAGE));
    }

    #[test]
    fn unit_select_latest_channel_release_prefers_channel_specific_tags() {
        let releases = vec![
            GitHubReleaseRecord {
                tag_name: "v0.3.0-beta.2".to_string(),
                prerelease: true,
                draft: false,
            },
            GitHubReleaseRecord {
                tag_name: "v0.2.1".to_string(),
                prerelease: false,
                draft: false,
            },
            GitHubReleaseRecord {
                tag_name: "v0.3.0-beta.1".to_string(),
                prerelease: true,
                draft: false,
            },
            GitHubReleaseRecord {
                tag_name: "v0.2.0".to_string(),
                prerelease: false,
                draft: true,
            },
        ];
        assert_eq!(
            select_latest_channel_release(ReleaseChannel::Stable, &releases),
            Some("v0.2.1".to_string())
        );
        assert_eq!(
            select_latest_channel_release(ReleaseChannel::Beta, &releases),
            Some("v0.3.0-beta.2".to_string())
        );
        assert_eq!(
            select_latest_channel_release(ReleaseChannel::Dev, &releases),
            Some("v0.3.0-beta.2".to_string())
        );

        let stable_only = vec![GitHubReleaseRecord {
            tag_name: "v1.0.0".to_string(),
            prerelease: false,
            draft: false,
        }];
        assert_eq!(
            select_latest_channel_release(ReleaseChannel::Beta, &stable_only),
            Some("v1.0.0".to_string())
        );
    }

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

    #[test]
    fn unit_resolve_channel_latest_or_unknown_returns_expected_values() {
        let releases = vec![
            GitHubReleaseRecord {
                tag_name: "v2.1.0-beta.1".to_string(),
                prerelease: true,
                draft: false,
            },
            GitHubReleaseRecord {
                tag_name: "v2.0.5".to_string(),
                prerelease: false,
                draft: false,
            },
        ];
        assert_eq!(
            resolve_channel_latest_or_unknown(ReleaseChannel::Stable, &releases),
            "v2.0.5"
        );
        assert_eq!(
            resolve_channel_latest_or_unknown(ReleaseChannel::Beta, &releases),
            "v2.1.0-beta.1"
        );
        assert_eq!(
            resolve_channel_latest_or_unknown(ReleaseChannel::Dev, &releases),
            "v2.1.0-beta.1"
        );
        assert_eq!(
            resolve_channel_latest_or_unknown(ReleaseChannel::Stable, &[]),
            "unknown"
        );
    }

    #[test]
    fn unit_compute_release_cache_age_counters_handles_fresh_boundary_and_stale() {
        let fresh = compute_release_cache_age_counters(10, 100);
        assert_eq!(
            fresh,
            ReleaseCacheAgeCounters {
                freshness: "fresh",
                next_refresh_in_ms: 90,
                stale_by_ms: 0
            }
        );

        let boundary = compute_release_cache_age_counters(100, 100);
        assert_eq!(
            boundary,
            ReleaseCacheAgeCounters {
                freshness: "fresh",
                next_refresh_in_ms: 0,
                stale_by_ms: 0
            }
        );

        let stale = compute_release_cache_age_counters(105, 100);
        assert_eq!(
            stale,
            ReleaseCacheAgeCounters {
                freshness: "stale",
                next_refresh_in_ms: 0,
                stale_by_ms: 5
            }
        );
    }

    #[test]
    fn unit_release_cache_expiration_helpers_handle_boundary_and_saturation() {
        assert_eq!(compute_release_cache_expires_at_unix_ms(100, 50), 150);
        assert_eq!(
            compute_release_cache_expires_at_unix_ms(u64::MAX - 1, 50),
            u64::MAX
        );
        assert!(!is_release_cache_expired(100, 100));
        assert!(is_release_cache_expired(101, 100));
    }

    #[test]
    fn unit_decide_release_cache_prune_handles_fresh_boundary_and_stale() {
        assert_eq!(
            decide_release_cache_prune(10, 100),
            ReleaseCachePruneDecision::KeepFresh
        );
        assert_eq!(
            decide_release_cache_prune(100, 100),
            ReleaseCachePruneDecision::KeepFresh
        );
        assert_eq!(
            decide_release_cache_prune(101, 100),
            ReleaseCachePruneDecision::RemoveStale
        );
    }

    #[test]
    fn unit_load_release_lookup_cache_for_prune_classifies_invalid_payload_and_schema() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("release-lookup-cache.json");

        std::fs::write(&path, "{malformed-json").expect("write malformed cache");
        let invalid_payload = load_release_lookup_cache_for_prune(&path);
        assert!(matches!(
            invalid_payload,
            ReleaseCachePruneLoadOutcome::RecoverableError {
                reason: ReleaseCachePruneRecoveryReason::InvalidPayload
            }
        ));

        std::fs::write(
            &path,
            r#"{"schema_version":99,"source_url":"https://example.invalid/releases","fetched_at_unix_ms":1,"releases":[]}"#,
        )
        .expect("write unsupported schema cache");
        let unsupported_schema = load_release_lookup_cache_for_prune(&path);
        assert!(matches!(
            unsupported_schema,
            ReleaseCachePruneLoadOutcome::RecoverableError {
                reason: ReleaseCachePruneRecoveryReason::UnsupportedSchema
            }
        ));
    }

    fn parse_u64_field(output: &str, key: &str) -> u64 {
        output
            .split_whitespace()
            .find_map(|token| {
                let (field_key, field_value) = token.split_once('=')?;
                if field_key == key {
                    return field_value.parse::<u64>().ok();
                }
                None
            })
            .unwrap_or_else(|| panic!("missing u64 field '{key}' in output: {output}"))
    }

    fn parse_bool_field(output: &str, key: &str) -> bool {
        output
            .split_whitespace()
            .find_map(|token| {
                let (field_key, field_value) = token.split_once('=')?;
                if field_key == key {
                    return field_value.parse::<bool>().ok();
                }
                None
            })
            .unwrap_or_else(|| panic!("missing bool field '{key}' in output: {output}"))
    }

    fn parse_string_field(output: &str, key: &str) -> String {
        output
            .split_whitespace()
            .find_map(|token| {
                let (field_key, field_value) = token.split_once('=')?;
                if field_key == key {
                    return Some(field_value.to_string());
                }
                None
            })
            .unwrap_or_else(|| panic!("missing string field '{key}' in output: {output}"))
    }

    #[test]
    fn functional_execute_release_channel_command_show_and_set_round_trip() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("release-channel.json");

        let initial = execute_release_channel_command("", &path);
        assert!(initial.contains("channel=stable"));
        assert!(initial.contains("source=default"));
        assert!(initial.contains("rollback_channel=none"));

        let set_output = execute_release_channel_command("set dev", &path);
        assert!(set_output.contains("channel=dev"));
        assert!(set_output.contains("previous_channel=stable"));
        assert!(set_output.contains("status=saved"));
        assert!(set_output.contains("rollback_channel=stable"));
        assert!(set_output.contains("rollback_version="));
        assert!(set_output.contains("rollback_reason=channel_switch"));

        let show = execute_release_channel_command("show", &path);
        assert!(show.contains("channel=dev"));
        assert!(show.contains("source=store"));
        assert!(show.contains("rollback_channel=stable"));
        assert!(show.contains("rollback_reason=channel_switch"));
    }

    #[test]
    fn functional_execute_release_channel_command_plan_dry_run_writes_update_state() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join(".tau/release-channel.json");
        let output = execute_release_channel_command_with_lookup_options(
            "plan --target v99.0.0 --dry-run",
            &path,
            "https://example.invalid/releases",
            RELEASE_LOOKUP_CACHE_TTL_MS,
        );

        assert!(output.contains("release channel plan:"));
        assert!(output.contains("action=blocked"));
        assert!(output.contains("dry_run=true"));
        assert!(output.contains("lookup_source=override"));
        assert!(output.contains("guard_code=major_version_jump_blocked"));
        assert!(output.contains("status=saved"));
        assert!(output.contains("state_path="));

        let state_path = temp.path().join(".tau/release-update-state.json");
        let state = load_release_update_state_file(&state_path)
            .expect("load update state")
            .expect("update state should exist");
        assert_eq!(state.schema_version, RELEASE_UPDATE_STATE_SCHEMA_VERSION);
        assert_eq!(state.target_version, "v99.0.0");
        assert_eq!(state.action, "blocked");
        assert!(state.dry_run);
    }

    #[test]
    fn integration_execute_release_channel_plan_and_apply_persists_lifecycle_state() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join(".tau/release-channel.json");
        save_release_channel_store(&path, ReleaseChannel::Beta).expect("save release channel");

        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/releases");
            then.status(200)
                .header("content-type", "application/json")
                .body(
                    r#"[{"tag_name":"v0.2.0-beta.1","prerelease":true,"draft":false},{"tag_name":"v0.1.0","prerelease":false,"draft":false}]"#,
                );
        });
        let url = format!("{}/releases", server.base_url());

        let plan = execute_release_channel_command_with_lookup_options(
            "plan",
            &path,
            &url,
            RELEASE_LOOKUP_CACHE_TTL_MS,
        );
        assert!(plan.contains("action=upgrade"));
        assert!(plan.contains("guard_code=ok"));
        assert!(plan.contains("status=saved"));

        let apply = execute_release_channel_command_with_lookup_options(
            "apply",
            &path,
            &url,
            RELEASE_LOOKUP_CACHE_TTL_MS,
        );
        assert!(apply.contains("action=upgrade"));
        assert!(apply.contains("status=applied_metadata"));
        assert!(apply.contains("guard_code=ok"));
        assert!(apply.contains("apply_attempts=1"));
        assert!(apply.contains("rollback_channel=beta"));
        assert!(apply.contains("rollback_reason=apply_upgrade"));
        assert_eq!(
            parse_string_field(&apply, "status"),
            "applied_metadata".to_string()
        );
        mock.assert_calls(1);

        let state_path = temp.path().join(".tau/release-update-state.json");
        let state = load_release_update_state_file(&state_path)
            .expect("load update state")
            .expect("update state should exist");
        assert_eq!(state.channel, ReleaseChannel::Beta);
        assert_eq!(state.target_version, "v0.2.0-beta.1");
        assert_eq!(state.action, "upgrade");
        assert_eq!(state.apply_attempts, 1);
        assert_eq!(state.last_apply_status.as_deref(), Some("applied_metadata"));

        let show = execute_release_channel_command("show", &path);
        assert!(show.contains("rollback_channel=beta"));
        assert!(show.contains("rollback_reason=apply_upgrade"));
    }

    #[test]
    fn functional_execute_release_channel_command_cache_show_and_clear_round_trip() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join(".tau/release-channel.json");
        let cache_path = temp.path().join(".tau/release-lookup-cache.json");

        let missing = execute_release_channel_command("cache show", &path);
        assert!(missing.contains("status=missing"));
        assert!(missing.contains(&format!("path={}", cache_path.display())));

        let releases = vec![GitHubReleaseRecord {
            tag_name: "v9.9.9".to_string(),
            prerelease: false,
            draft: false,
        }];
        save_release_lookup_cache(
            &cache_path,
            "https://example.invalid/releases",
            current_unix_timestamp_ms(),
            &releases,
        )
        .expect("save cache");

        let present = execute_release_channel_command("cache show", &path);
        assert!(present.contains("status=present"));
        assert!(present.contains("entries=1"));
        assert!(present.contains("ttl_ms=900000"));
        assert!(present.contains("freshness=fresh"));
        assert!(present.contains("next_refresh_in_ms="));
        assert!(present.contains("stale_by_ms=0"));
        assert!(present.contains("expires_at_unix_ms="));
        assert!(present.contains("is_expired=false"));
        assert!(present.contains("source_url=https://example.invalid/releases"));
        assert!(present.contains("stable_latest=v9.9.9"));
        assert!(present.contains("beta_latest=v9.9.9"));
        assert!(present.contains("dev_latest=v9.9.9"));
        assert!(parse_u64_field(&present, "next_refresh_in_ms") <= RELEASE_LOOKUP_CACHE_TTL_MS);
        let fetched_at = parse_u64_field(&present, "fetched_at_unix_ms");
        let expires_at = parse_u64_field(&present, "expires_at_unix_ms");
        assert_eq!(
            expires_at,
            compute_release_cache_expires_at_unix_ms(fetched_at, RELEASE_LOOKUP_CACHE_TTL_MS)
        );
        assert!(!parse_bool_field(&present, "is_expired"));

        let cleared = execute_release_channel_command("cache clear", &path);
        assert!(cleared.contains("status=removed"));
        assert!(!cache_path.exists());

        let cleared_again = execute_release_channel_command("cache clear", &path);
        assert!(cleared_again.contains("status=already_missing"));
    }

    #[test]
    fn functional_execute_release_channel_command_cache_refresh_and_show_round_trip() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join(".tau/release-channel.json");
        let cache_path = temp.path().join(".tau/release-lookup-cache.json");
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/releases");
            then.status(200)
                .header("content-type", "application/json")
                .body(
                    r#"[{"tag_name":"v9.0.0-beta.2","prerelease":true,"draft":false},{"tag_name":"v8.9.4","prerelease":false,"draft":false}]"#,
                );
        });
        let url = format!("{}/releases", server.base_url());

        let refreshed = execute_release_channel_command_with_lookup_options(
            "cache refresh",
            &path,
            &url,
            RELEASE_LOOKUP_CACHE_TTL_MS,
        );
        assert!(refreshed.contains("status=refreshed"));
        assert!(refreshed.contains("entries=2"));
        assert!(refreshed.contains(&format!("path={}", cache_path.display())));
        assert!(refreshed.contains(&format!("source_url={url}")));

        let show = execute_release_channel_command_with_lookup_options(
            "cache show",
            &path,
            &url,
            RELEASE_LOOKUP_CACHE_TTL_MS,
        );
        assert!(show.contains("status=present"));
        assert!(show.contains("entries=2"));
        assert!(show.contains("freshness=fresh"));
        assert!(show.contains("next_refresh_in_ms="));
        assert!(show.contains("stale_by_ms=0"));
        assert!(show.contains("expires_at_unix_ms="));
        assert!(show.contains("is_expired=false"));
        assert!(show.contains(&format!("source_url={url}")));
        assert!(show.contains("stable_latest=v8.9.4"));
        assert!(show.contains("beta_latest=v9.0.0-beta.2"));
        assert!(show.contains("dev_latest=v9.0.0-beta.2"));
        assert!(parse_u64_field(&show, "next_refresh_in_ms") <= RELEASE_LOOKUP_CACHE_TTL_MS);
        let fetched_at = parse_u64_field(&show, "fetched_at_unix_ms");
        let expires_at = parse_u64_field(&show, "expires_at_unix_ms");
        assert_eq!(
            expires_at,
            compute_release_cache_expires_at_unix_ms(fetched_at, RELEASE_LOOKUP_CACHE_TTL_MS)
        );
        assert!(!parse_bool_field(&show, "is_expired"));
        mock.assert_calls(1);
    }

    #[test]
    fn functional_execute_release_channel_command_cache_prune_keeps_fresh_cache() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join(".tau/release-channel.json");
        let cache_path = temp.path().join(".tau/release-lookup-cache.json");
        let releases = vec![GitHubReleaseRecord {
            tag_name: "v11.0.0".to_string(),
            prerelease: false,
            draft: false,
        }];
        save_release_lookup_cache(
            &cache_path,
            "https://example.invalid/releases",
            current_unix_timestamp_ms(),
            &releases,
        )
        .expect("save cache");

        let output = execute_release_channel_command("cache prune", &path);
        assert!(output.contains("status=kept"));
        assert!(output.contains("reason=fresh"));
        assert!(output.contains("freshness=fresh"));
        assert!(output.contains("next_refresh_in_ms="));
        assert!(output.contains("stale_by_ms=0"));
        assert!(output.contains("is_expired=false"));
        assert!(cache_path.exists());
    }

    #[test]
    fn functional_execute_release_channel_command_cache_prune_recovers_invalid_payload() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join(".tau/release-channel.json");
        let cache_path = temp.path().join(".tau/release-lookup-cache.json");
        let parent = cache_path.parent().expect("cache parent");
        std::fs::create_dir_all(parent).expect("create cache dir");
        std::fs::write(&cache_path, "{malformed-json").expect("write malformed cache");

        let output = execute_release_channel_command("cache prune", &path);
        assert!(output.contains("status=removed"));
        assert!(output.contains("reason=invalid_payload"));
        assert!(output.contains("recovery_action=removed_invalid_cache"));
        assert!(!cache_path.exists());
    }

    #[test]
    fn functional_render_release_channel_check_reports_update_available() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("release-channel.json");
        let output = render_release_channel_check_with_lookup(&path, "0.1.0", |_| {
            Ok(LatestChannelReleaseResolution {
                latest: Some("v0.2.0".into()),
                source: ReleaseLookupSource::Live,
            })
        });
        assert!(output.contains("channel=stable"));
        assert!(output.contains("channel_source=default"));
        assert!(output.contains("current=0.1.0"));
        assert!(output.contains("latest=v0.2.0"));
        assert!(output.contains("status=update_available"));
        assert!(output.contains("lookup_source=live"));
    }

    #[test]
    fn functional_render_release_channel_check_reports_up_to_date_when_current_is_newer() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join(".tau/release-channel.json");
        save_release_channel_store(&path, ReleaseChannel::Beta).expect("save release channel");

        let output = render_release_channel_check_with_lookup(&path, "0.3.0", |_| {
            Ok(LatestChannelReleaseResolution {
                latest: Some("v0.2.5".into()),
                source: ReleaseLookupSource::CacheFresh,
            })
        });
        assert!(output.contains("channel=beta"));
        assert!(output.contains("channel_source=store"));
        assert!(output.contains("status=up_to_date"));
        assert!(output.contains("lookup_source=cache_fresh"));
    }

    #[test]
    fn integration_execute_release_channel_check_uses_cache_after_live_lookup() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join(".tau/release-channel.json");
        let cache_path = temp.path().join(".tau/release-lookup-cache.json");
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/releases");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"[{"tag_name":"v2.0.0-beta.1","prerelease":true,"draft":false},{"tag_name":"v1.9.0","prerelease":false,"draft":false}]"#);
        });
        let url = format!("{}/releases", server.base_url());

        let first = execute_release_channel_check_with_lookup_options(
            &path,
            "0.1.0",
            &url,
            &cache_path,
            RELEASE_LOOKUP_CACHE_TTL_MS,
        );
        assert!(first.contains("status=update_available"));
        assert!(first.contains("lookup_source=live"));

        let second = execute_release_channel_check_with_lookup_options(
            &path,
            "0.1.0",
            &url,
            &cache_path,
            RELEASE_LOOKUP_CACHE_TTL_MS,
        );
        assert!(second.contains("status=update_available"));
        assert!(second.contains("lookup_source=cache_fresh"));
        mock.assert_calls(1);
    }

    #[test]
    fn integration_execute_release_channel_command_cache_refresh_persists_lookup_cache() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join(".tau/release-channel.json");
        let cache_path = temp.path().join(".tau/release-lookup-cache.json");
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/releases");
            then.status(200)
                .header("content-type", "application/json")
                .body(
                    r#"[{"tag_name":"v10.1.0","prerelease":false,"draft":false},{"tag_name":"v10.2.0-beta.1","prerelease":true,"draft":false}]"#,
                );
        });
        let url = format!("{}/releases", server.base_url());

        let output = execute_release_channel_command_with_lookup_options(
            "cache refresh",
            &path,
            &url,
            RELEASE_LOOKUP_CACHE_TTL_MS,
        );
        assert!(output.contains("status=refreshed"));
        assert!(output.contains("entries=2"));
        mock.assert_calls(1);

        let cached = load_release_lookup_cache(&cache_path, &url)
            .expect("load refreshed cache")
            .expect("refreshed cache should exist");
        assert_eq!(cached.schema_version, RELEASE_LOOKUP_CACHE_SCHEMA_VERSION);
        assert_eq!(cached.source_url, url);
        assert_eq!(cached.releases.len(), 2);
        assert_eq!(cached.releases[0].tag_name, "v10.1.0");

        let show = execute_release_channel_command_with_lookup_options(
            "cache show",
            &path,
            &url,
            RELEASE_LOOKUP_CACHE_TTL_MS,
        );
        assert!(show.contains("next_refresh_in_ms="));
        assert!(show.contains("stale_by_ms=0"));
        assert!(show.contains("expires_at_unix_ms="));
        assert!(show.contains("is_expired=false"));
        assert!(show.contains("stable_latest=v10.1.0"));
        assert!(show.contains("beta_latest=v10.2.0-beta.1"));
        assert!(show.contains("dev_latest=v10.2.0-beta.1"));
        assert!(parse_u64_field(&show, "next_refresh_in_ms") <= RELEASE_LOOKUP_CACHE_TTL_MS);
        assert!(!parse_bool_field(&show, "is_expired"));
    }

    #[test]
    fn integration_execute_release_channel_command_cache_refresh_then_prune_keeps_fresh_cache() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join(".tau/release-channel.json");
        let cache_path = temp.path().join(".tau/release-lookup-cache.json");
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/releases");
            then.status(200)
                .header("content-type", "application/json")
                .body(
                    r#"[{"tag_name":"v12.0.0","prerelease":false,"draft":false},{"tag_name":"v12.1.0-beta.1","prerelease":true,"draft":false}]"#,
                );
        });
        let url = format!("{}/releases", server.base_url());

        let refreshed = execute_release_channel_command_with_lookup_options(
            "cache refresh",
            &path,
            &url,
            RELEASE_LOOKUP_CACHE_TTL_MS,
        );
        assert!(refreshed.contains("status=refreshed"));
        assert!(cache_path.exists());

        let pruned = execute_release_channel_command_with_lookup_options(
            "cache prune",
            &path,
            &url,
            RELEASE_LOOKUP_CACHE_TTL_MS,
        );
        assert!(pruned.contains("status=kept"));
        assert!(pruned.contains("reason=fresh"));
        assert!(pruned.contains("freshness=fresh"));
        assert!(pruned.contains("is_expired=false"));
        assert!(cache_path.exists());
        mock.assert_calls(1);
    }

    #[test]
    fn integration_execute_release_channel_command_refresh_corrupt_prune_refresh_recovers() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join(".tau/release-channel.json");
        let cache_path = temp.path().join(".tau/release-lookup-cache.json");
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/releases");
            then.status(200)
                .header("content-type", "application/json")
                .body(
                    r#"[{"tag_name":"v14.0.0","prerelease":false,"draft":false},{"tag_name":"v14.1.0-beta.1","prerelease":true,"draft":false}]"#,
                );
        });
        let url = format!("{}/releases", server.base_url());

        let refreshed = execute_release_channel_command_with_lookup_options(
            "cache refresh",
            &path,
            &url,
            RELEASE_LOOKUP_CACHE_TTL_MS,
        );
        assert!(refreshed.contains("status=refreshed"));
        assert!(cache_path.exists());

        std::fs::write(&cache_path, "{malformed-json").expect("write malformed cache");
        let pruned = execute_release_channel_command_with_lookup_options(
            "cache prune",
            &path,
            &url,
            RELEASE_LOOKUP_CACHE_TTL_MS,
        );
        assert!(pruned.contains("status=removed"));
        assert!(pruned.contains("reason=invalid_payload"));
        assert!(pruned.contains("recovery_action=removed_invalid_cache"));
        assert!(!cache_path.exists());

        let refreshed_again = execute_release_channel_command_with_lookup_options(
            "cache refresh",
            &path,
            &url,
            RELEASE_LOOKUP_CACHE_TTL_MS,
        );
        assert!(refreshed_again.contains("status=refreshed"));
        assert!(cache_path.exists());
        mock.assert_calls(2);
    }

    #[test]
    fn integration_save_and_load_release_channel_store_round_trip() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join(".tau/release-channel.json");
        save_release_channel_store(&path, ReleaseChannel::Beta).expect("save release channel");
        let loaded = load_release_channel_store(&path).expect("load release channel");
        assert_eq!(loaded, Some(ReleaseChannel::Beta));
    }

    #[test]
    fn integration_fetch_release_records_parses_github_style_response() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/releases")
                .header("user-agent", RELEASE_LOOKUP_USER_AGENT);
            then.status(200)
                .header("content-type", "application/json")
                .body(
                    r#"[{"tag_name":"v1.2.3","prerelease":false,"draft":false},{"tag_name":"v1.3.0-beta.1","prerelease":true,"draft":false}]"#,
                );
        });

        let url = format!("{}/releases", server.base_url());
        let records = fetch_release_records(&url).expect("fetch release records");
        mock.assert();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].tag_name, "v1.2.3");
        assert!(!records[0].prerelease);
    }

    #[test]
    fn integration_resolve_latest_channel_release_applies_channel_routing() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/releases");
            then.status(200)
                .header("content-type", "application/json")
                .body(
                    r#"[{"tag_name":"v2.0.0-beta.1","prerelease":true,"draft":false},{"tag_name":"v1.9.0","prerelease":false,"draft":false}]"#,
                );
        });
        let url = format!("{}/releases", server.base_url());

        let stable = resolve_latest_channel_release(ReleaseChannel::Stable, &url)
            .expect("stable resolution should succeed");
        let beta = resolve_latest_channel_release(ReleaseChannel::Beta, &url)
            .expect("beta resolution should succeed");
        mock.assert_calls(2);

        assert_eq!(stable.as_deref(), Some("v1.9.0"));
        assert_eq!(beta.as_deref(), Some("v2.0.0-beta.1"));
    }

    #[test]
    fn functional_resolve_latest_channel_release_cached_uses_fresh_cache_without_network() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cache_path = temp.path().join("release-lookup-cache.json");
        let url = "http://127.0.0.1:9/releases";
        let releases = vec![GitHubReleaseRecord {
            tag_name: "v9.9.9".to_string(),
            prerelease: false,
            draft: false,
        }];
        save_release_lookup_cache(&cache_path, url, current_unix_timestamp_ms(), &releases)
            .expect("save cache");

        let resolution = resolve_latest_channel_release_cached(
            ReleaseChannel::Stable,
            url,
            &cache_path,
            RELEASE_LOOKUP_CACHE_TTL_MS,
        )
        .expect("resolve from cache");
        assert_eq!(resolution.source, ReleaseLookupSource::CacheFresh);
        assert_eq!(resolution.latest.as_deref(), Some("v9.9.9"));
    }

    #[test]
    fn integration_resolve_latest_channel_release_cached_fetches_live_and_persists_cache() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cache_path = temp.path().join("release-lookup-cache.json");
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/releases");
            then.status(200).header("content-type", "application/json").body(
                r#"[{"tag_name":"v3.1.0-beta.1","prerelease":true,"draft":false},{"tag_name":"v3.0.2","prerelease":false,"draft":false}]"#,
            );
        });
        let url = format!("{}/releases", server.base_url());

        let first = resolve_latest_channel_release_cached(
            ReleaseChannel::Stable,
            &url,
            &cache_path,
            RELEASE_LOOKUP_CACHE_TTL_MS,
        )
        .expect("first live resolve");
        assert_eq!(first.source, ReleaseLookupSource::Live);
        assert_eq!(first.latest.as_deref(), Some("v3.0.2"));

        let second = resolve_latest_channel_release_cached(
            ReleaseChannel::Beta,
            &url,
            &cache_path,
            RELEASE_LOOKUP_CACHE_TTL_MS,
        )
        .expect("second cached resolve");
        assert_eq!(second.source, ReleaseLookupSource::CacheFresh);
        assert_eq!(second.latest.as_deref(), Some("v3.1.0-beta.1"));
        mock.assert_calls(1);

        let cached = load_release_lookup_cache(&cache_path, &url)
            .expect("load cached payload")
            .expect("cached payload should exist");
        assert_eq!(cached.schema_version, RELEASE_LOOKUP_CACHE_SCHEMA_VERSION);
        assert_eq!(cached.source_url, url);
        assert_eq!(cached.releases.len(), 2);
    }

    #[test]
    fn regression_load_release_channel_store_rejects_invalid_schema_and_payload() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("release-channel.json");
        std::fs::write(&path, r#"{"schema_version":99,"release_channel":"stable"}"#)
            .expect("write invalid schema");
        let schema_error = load_release_channel_store(&path).expect_err("schema should fail");
        assert!(schema_error
            .to_string()
            .contains("unsupported release channel schema_version"));

        std::fs::write(&path, "{invalid-json").expect("write malformed json");
        let parse_error = load_release_channel_store(&path).expect_err("parse should fail");
        assert!(parse_error
            .to_string()
            .contains("failed to parse release channel file"));
    }

    #[test]
    fn regression_load_release_channel_store_supports_legacy_schema_version_one() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("release-channel.json");
        std::fs::write(&path, r#"{"schema_version":1,"release_channel":"dev"}"#)
            .expect("write legacy payload");

        let channel = load_release_channel_store(&path).expect("load release channel");
        assert_eq!(channel, Some(ReleaseChannel::Dev));

        let show = execute_release_channel_command("show", &path);
        assert!(show.contains("channel=dev"));
        assert!(show.contains("rollback_channel=none"));
    }

    #[test]
    fn regression_fetch_release_records_reports_http_and_payload_failures() {
        let error_server = MockServer::start();
        let error_mock = error_server.mock(|when, then| {
            when.method(GET).path("/releases");
            then.status(500);
        });
        let error_url = format!("{}/releases", error_server.base_url());
        let http_error = fetch_release_records(&error_url).expect_err("http error should fail");
        error_mock.assert();
        assert!(http_error.to_string().contains("returned status 500"));

        let invalid_server = MockServer::start();
        let invalid_mock = invalid_server.mock(|when, then| {
            when.method(GET).path("/releases");
            then.status(200)
                .header("content-type", "application/json")
                .body("{\"tag_name\":\"not-an-array\"}");
        });
        let invalid_url = format!("{}/releases", invalid_server.base_url());
        let parse_error =
            fetch_release_records(&invalid_url).expect_err("invalid payload should fail");
        invalid_mock.assert();
        assert!(parse_error
            .to_string()
            .contains("failed to parse release lookup response"));
    }

    #[test]
    fn regression_execute_release_channel_apply_blocks_prerelease_target_on_stable() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join(".tau/release-channel.json");
        save_release_channel_store(&path, ReleaseChannel::Stable).expect("save release channel");

        let output = execute_release_channel_command_with_lookup_options(
            "apply --target v1.2.0-beta.1",
            &path,
            "https://example.invalid/releases",
            RELEASE_LOOKUP_CACHE_TTL_MS,
        );
        assert!(output.contains("action=blocked"));
        assert!(output.contains("status=blocked"));
        assert!(output.contains("guard_code=stable_prerelease_disallowed"));
    }

    #[test]
    fn regression_execute_release_channel_plan_fails_closed_on_malformed_update_state() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join(".tau/release-channel.json");
        let state_path = temp.path().join(".tau/release-update-state.json");
        std::fs::create_dir_all(state_path.parent().expect("state parent"))
            .expect("create state dir");
        std::fs::write(&state_path, "{invalid-json").expect("write malformed update state");

        let output = execute_release_channel_command_with_lookup_options(
            "plan --target v0.1.1",
            &path,
            "https://example.invalid/releases",
            RELEASE_LOOKUP_CACHE_TTL_MS,
        );
        assert!(output.contains("release channel error:"));
        assert!(output.contains("failed to parse release update state"));
    }

    #[test]
    fn regression_resolve_latest_channel_release_cached_falls_back_to_stale_cache_on_lookup_error()
    {
        let temp = tempfile::tempdir().expect("tempdir");
        let cache_path = temp.path().join("release-lookup-cache.json");
        let url = "http://127.0.0.1:9/releases";
        let stale_time =
            current_unix_timestamp_ms().saturating_sub(RELEASE_LOOKUP_CACHE_TTL_MS + 5_000);
        let releases = vec![GitHubReleaseRecord {
            tag_name: "v4.2.0".to_string(),
            prerelease: false,
            draft: false,
        }];
        save_release_lookup_cache(&cache_path, url, stale_time, &releases)
            .expect("save stale cache");

        let resolution = resolve_latest_channel_release_cached(
            ReleaseChannel::Stable,
            url,
            &cache_path,
            RELEASE_LOOKUP_CACHE_TTL_MS,
        )
        .expect("stale fallback should succeed");
        assert_eq!(resolution.source, ReleaseLookupSource::CacheStaleFallback);
        assert_eq!(resolution.latest.as_deref(), Some("v4.2.0"));
    }

    #[test]
    fn regression_resolve_latest_channel_release_cached_ignores_invalid_cache_and_refetches_live() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cache_path = temp.path().join("release-lookup-cache.json");
        std::fs::write(
            &cache_path,
            r#"{"schema_version":99,"source_url":"https://invalid","fetched_at_unix_ms":1,"releases":[]}"#,
        )
        .expect("write invalid cache");

        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/releases");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"[{"tag_name":"v5.0.0","prerelease":false,"draft":false}]"#);
        });
        let url = format!("{}/releases", server.base_url());
        let resolution = resolve_latest_channel_release_cached(
            ReleaseChannel::Stable,
            &url,
            &cache_path,
            RELEASE_LOOKUP_CACHE_TTL_MS,
        )
        .expect("live lookup should recover from invalid cache");
        assert_eq!(resolution.source, ReleaseLookupSource::Live);
        assert_eq!(resolution.latest.as_deref(), Some("v5.0.0"));
        mock.assert_calls(1);
    }

    #[test]
    fn regression_render_release_channel_check_reports_lookup_errors_without_panicking() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("release-channel.json");
        let output = render_release_channel_check_with_lookup(&path, "0.1.0", |_| {
            Err(anyhow::anyhow!("lookup backend unavailable"))
        });
        assert!(output.contains("latest=unknown"));
        assert!(output.contains("status=unknown"));
        assert!(output.contains("lookup_source=unknown"));
        assert!(output.contains("error=lookup backend unavailable"));
    }

    #[test]
    fn regression_execute_release_channel_command_cache_show_reports_parse_errors() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join(".tau/release-channel.json");
        let cache_path = temp.path().join(".tau/release-lookup-cache.json");
        let parent = cache_path.parent().expect("cache parent");
        std::fs::create_dir_all(parent).expect("create cache dir");
        std::fs::write(&cache_path, "{malformed-json").expect("write malformed cache");

        let output = execute_release_channel_command("cache show", &path);
        assert!(output.contains("release channel error:"));
        assert!(output.contains("failed to parse release lookup cache"));
    }

    #[test]
    fn regression_execute_release_channel_command_cache_refresh_reports_lookup_errors() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join(".tau/release-channel.json");
        let cache_path = temp.path().join(".tau/release-lookup-cache.json");
        let original_url = "https://example.invalid/releases";
        let original_releases = vec![GitHubReleaseRecord {
            tag_name: "v7.0.1".to_string(),
            prerelease: false,
            draft: false,
        }];
        save_release_lookup_cache(
            &cache_path,
            original_url,
            current_unix_timestamp_ms(),
            &original_releases,
        )
        .expect("seed cache");

        let output = execute_release_channel_command_with_lookup_options(
            "cache refresh",
            &path,
            "http://127.0.0.1:9/releases",
            RELEASE_LOOKUP_CACHE_TTL_MS,
        );
        assert!(output.contains("release channel error:"));
        assert!(output.contains("failed to fetch release metadata"));

        let cached = load_release_lookup_cache_file(&cache_path)
            .expect("load seeded cache")
            .expect("cache should still exist");
        assert_eq!(cached.source_url, original_url);
        assert_eq!(cached.releases.len(), 1);
        assert_eq!(cached.releases[0].tag_name, "v7.0.1");
    }

    #[test]
    fn regression_execute_release_channel_command_cache_show_reports_stale_freshness() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join(".tau/release-channel.json");
        let cache_path = temp.path().join(".tau/release-lookup-cache.json");
        let stale_time =
            current_unix_timestamp_ms().saturating_sub(RELEASE_LOOKUP_CACHE_TTL_MS + 5_000);
        let releases = vec![GitHubReleaseRecord {
            tag_name: "v8.8.8".to_string(),
            prerelease: false,
            draft: false,
        }];
        save_release_lookup_cache(
            &cache_path,
            "https://example.invalid/releases",
            stale_time,
            &releases,
        )
        .expect("save stale cache");

        let output = execute_release_channel_command("cache show", &path);
        assert!(output.contains("status=present"));
        assert!(output.contains("freshness=stale"));
        assert!(output.contains("ttl_ms=900000"));
        assert!(output.contains("next_refresh_in_ms=0"));
        assert!(output.contains("stale_by_ms="));
        assert!(output.contains("expires_at_unix_ms="));
        assert!(output.contains("is_expired=true"));
        assert!(output.contains("stable_latest=v8.8.8"));
        assert!(output.contains("beta_latest=v8.8.8"));
        assert!(output.contains("dev_latest=v8.8.8"));
        assert!(parse_u64_field(&output, "stale_by_ms") > 0);
        assert!(parse_bool_field(&output, "is_expired"));
    }

    #[test]
    fn regression_execute_release_channel_command_cache_prune_removes_stale_cache() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join(".tau/release-channel.json");
        let cache_path = temp.path().join(".tau/release-lookup-cache.json");
        let stale_time =
            current_unix_timestamp_ms().saturating_sub(RELEASE_LOOKUP_CACHE_TTL_MS + 5_000);
        let releases = vec![GitHubReleaseRecord {
            tag_name: "v13.1.1".to_string(),
            prerelease: false,
            draft: false,
        }];
        save_release_lookup_cache(
            &cache_path,
            "https://example.invalid/releases",
            stale_time,
            &releases,
        )
        .expect("save stale cache");

        let output = execute_release_channel_command("cache prune", &path);
        assert!(output.contains("status=removed"));
        assert!(output.contains("reason=stale"));
        assert!(output.contains("freshness=stale"));
        assert!(output.contains("next_refresh_in_ms=0"));
        assert!(output.contains("stale_by_ms="));
        assert!(output.contains("is_expired=true"));
        assert!(parse_u64_field(&output, "stale_by_ms") > 0);
        assert!(parse_bool_field(&output, "is_expired"));
        assert!(!cache_path.exists());
    }

    #[test]
    fn regression_execute_release_channel_command_cache_prune_invalid_payload_no_error_status() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join(".tau/release-channel.json");
        let cache_path = temp.path().join(".tau/release-lookup-cache.json");
        let parent = cache_path.parent().expect("cache parent");
        std::fs::create_dir_all(parent).expect("create cache dir");
        std::fs::write(&cache_path, "{malformed-json").expect("write malformed cache");

        let output = execute_release_channel_command("cache prune", &path);
        assert!(output.contains("status=removed"));
        assert!(output.contains("reason=invalid_payload"));
        assert!(output.contains("recovery_action=removed_invalid_cache"));
        assert!(!output.contains("release channel error:"));
        assert!(!cache_path.exists());
    }

    #[test]
    fn regression_execute_release_channel_command_cache_show_reports_unknown_channel_latest_for_empty_cache(
    ) {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join(".tau/release-channel.json");
        let cache_path = temp.path().join(".tau/release-lookup-cache.json");
        let stale_time =
            current_unix_timestamp_ms().saturating_sub(RELEASE_LOOKUP_CACHE_TTL_MS + 5_000);
        save_release_lookup_cache(
            &cache_path,
            "https://example.invalid/releases",
            stale_time,
            &[],
        )
        .expect("save empty cache");

        let output = execute_release_channel_command("cache show", &path);
        assert!(output.contains("status=present"));
        assert!(output.contains("entries=0"));
        assert!(output.contains("stable_latest=unknown"));
        assert!(output.contains("beta_latest=unknown"));
        assert!(output.contains("dev_latest=unknown"));
        assert!(output.contains("next_refresh_in_ms=0"));
        assert!(parse_u64_field(&output, "stale_by_ms") > 0);
        assert!(output.contains("expires_at_unix_ms="));
        assert!(output.contains("is_expired=true"));
        assert!(parse_bool_field(&output, "is_expired"));
    }

    #[test]
    fn regression_execute_release_channel_check_falls_back_to_stale_cache() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join(".tau/release-channel.json");
        let cache_path = temp.path().join(".tau/release-lookup-cache.json");
        let lookup_url = "http://127.0.0.1:9/releases";
        let stale_time =
            current_unix_timestamp_ms().saturating_sub(RELEASE_LOOKUP_CACHE_TTL_MS + 5_000);
        let releases = vec![GitHubReleaseRecord {
            tag_name: "v4.2.0".to_string(),
            prerelease: false,
            draft: false,
        }];
        save_release_lookup_cache(&cache_path, lookup_url, stale_time, &releases)
            .expect("save stale cache");

        let output = execute_release_channel_check_with_lookup_options(
            &path,
            "0.1.0",
            lookup_url,
            &cache_path,
            RELEASE_LOOKUP_CACHE_TTL_MS,
        );
        assert!(output.contains("latest=v4.2.0"));
        assert!(output.contains("status=update_available"));
        assert!(output.contains("lookup_source=cache_stale_fallback"));
    }
}
