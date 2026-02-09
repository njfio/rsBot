use super::*;

pub(crate) const RELEASE_CHANNEL_USAGE: &str =
    "usage: /release-channel [show|set <stable|beta|dev>|check|cache <show|clear>]";
pub(crate) const RELEASE_CHANNEL_SCHEMA_VERSION: u32 = 1;
pub(crate) const RELEASE_LOOKUP_CACHE_SCHEMA_VERSION: u32 = 1;
pub(crate) const RELEASE_LOOKUP_CACHE_TTL_MS: u64 = 15 * 60 * 1_000;
const RELEASE_LOOKUP_URL: &str = "https://api.github.com/repos/njfio/Tau/releases?per_page=30";
const RELEASE_LOOKUP_USER_AGENT: &str = "tau-coding-agent/release-channel-check";
const RELEASE_LOOKUP_TIMEOUT_MS: u64 = 8_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ReleaseChannel {
    Stable,
    Beta,
    Dev,
}

impl ReleaseChannel {
    pub(crate) fn as_str(self) -> &'static str {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReleaseChannelCommand {
    Show,
    Set(ReleaseChannel),
    Check,
    CacheShow,
    CacheClear,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ReleaseChannelStoreFile {
    pub(crate) schema_version: u32,
    pub(crate) release_channel: ReleaseChannel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct GitHubReleaseRecord {
    pub(crate) tag_name: String,
    #[serde(default)]
    pub(crate) prerelease: bool,
    #[serde(default)]
    pub(crate) draft: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ReleaseLookupCacheFile {
    schema_version: u32,
    source_url: String,
    fetched_at_unix_ms: u64,
    releases: Vec<GitHubReleaseRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReleaseLookupSource {
    Live,
    CacheFresh,
    CacheStaleFallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LatestChannelReleaseResolution {
    pub(crate) latest: Option<String>,
    pub(crate) source: ReleaseLookupSource,
}

pub(crate) fn default_release_channel_path() -> Result<PathBuf> {
    Ok(std::env::current_dir()
        .context("failed to resolve current working directory")?
        .join(".tau")
        .join("release-channel.json"))
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

    if tokens.len() == 2 && tokens[0] == "set" {
        let channel = tokens[1].parse::<ReleaseChannel>()?;
        return Ok(ReleaseChannelCommand::Set(channel));
    }

    if tokens.len() == 2 && tokens[0] == "cache" {
        return match tokens[1] {
            "show" => Ok(ReleaseChannelCommand::CacheShow),
            "clear" => Ok(ReleaseChannelCommand::CacheClear),
            _ => bail!("{RELEASE_CHANNEL_USAGE}"),
        };
    }

    bail!("{RELEASE_CHANNEL_USAGE}");
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

pub(crate) fn load_release_channel_store(path: &Path) -> Result<Option<ReleaseChannel>> {
    if !path.exists() {
        return Ok(None);
    }

    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read release channel file {}", path.display()))?;
    let parsed = serde_json::from_str::<ReleaseChannelStoreFile>(&raw)
        .with_context(|| format!("failed to parse release channel file {}", path.display()))?;
    if parsed.schema_version != RELEASE_CHANNEL_SCHEMA_VERSION {
        bail!(
            "unsupported release channel schema_version {} in {} (expected {})",
            parsed.schema_version,
            path.display(),
            RELEASE_CHANNEL_SCHEMA_VERSION
        );
    }
    Ok(Some(parsed.release_channel))
}

pub(crate) fn save_release_channel_store(path: &Path, channel: ReleaseChannel) -> Result<()> {
    let payload = ReleaseChannelStoreFile {
        schema_version: RELEASE_CHANNEL_SCHEMA_VERSION,
        release_channel: channel,
    };
    let mut encoded =
        serde_json::to_string_pretty(&payload).context("failed to encode release channel store")?;
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

fn select_latest_channel_release(
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

pub(crate) fn compare_versions(current: &str, latest: &str) -> Option<std::cmp::Ordering> {
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

fn fetch_release_records(url: &str) -> Result<Vec<GitHubReleaseRecord>> {
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

fn load_release_lookup_cache_file(path: &Path) -> Result<Option<ReleaseLookupCacheFile>> {
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

fn load_release_lookup_cache(path: &Path, url: &str) -> Result<Option<ReleaseLookupCacheFile>> {
    let Some(parsed) = load_release_lookup_cache_file(path)? else {
        return Ok(None);
    };
    if parsed.source_url != url {
        return Ok(None);
    }
    Ok(Some(parsed))
}

fn save_release_lookup_cache(
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

pub(crate) fn resolve_latest_channel_release(
    channel: ReleaseChannel,
    url: &str,
) -> Result<Option<String>> {
    let releases = fetch_release_records(url)?;
    Ok(select_latest_channel_release(channel, &releases))
}

pub(crate) fn resolve_latest_channel_release_cached(
    channel: ReleaseChannel,
    url: &str,
    cache_path: &Path,
    cache_ttl_ms: u64,
) -> Result<LatestChannelReleaseResolution> {
    let now_ms = current_unix_timestamp_ms();
    let mut stale_cache_releases: Option<Vec<GitHubReleaseRecord>> = None;

    if let Ok(Some(cache)) = load_release_lookup_cache(cache_path, url) {
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
            let _ = save_release_lookup_cache(cache_path, url, now_ms, &releases);
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

pub(crate) fn release_lookup_url() -> &'static str {
    RELEASE_LOOKUP_URL
}

fn render_release_channel_check_with_lookup<F>(
    path: &Path,
    current_version: &str,
    lookup: F,
) -> String
where
    F: Fn(ReleaseChannel) -> Result<Option<String>>,
{
    let (channel, channel_source) = match load_release_channel_store(path) {
        Ok(Some(channel)) => (channel, "store"),
        Ok(None) => (ReleaseChannel::Stable, "default"),
        Err(error) => {
            return format!(
                "release channel error: path={} error={error}",
                path.display()
            );
        }
    };

    match lookup(channel) {
        Ok(Some(latest)) => {
            let status = match compare_versions(current_version, &latest) {
                Some(std::cmp::Ordering::Less) => "update_available",
                Some(std::cmp::Ordering::Equal | std::cmp::Ordering::Greater) => "up_to_date",
                None => "unknown",
            };
            format!(
                "release channel check: path={} channel={} channel_source={} current={} latest={} status={} source=github",
                path.display(),
                channel,
                channel_source,
                current_version,
                latest,
                status
            )
        }
        Ok(None) => format!(
            "release channel check: path={} channel={} channel_source={} current={} latest=unknown status=unknown source=github error=no_release_records",
            path.display(),
            channel,
            channel_source,
            current_version
        ),
        Err(error) => format!(
            "release channel check: path={} channel={} channel_source={} current={} latest=unknown status=unknown source=github error={error}",
            path.display(),
            channel,
            channel_source,
            current_version
        ),
    }
}

pub(crate) fn execute_release_channel_command(command_args: &str, path: &Path) -> String {
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
        ReleaseChannelCommand::Show => match load_release_channel_store(path) {
            Ok(Some(channel)) => format!(
                "release channel: path={} channel={} source=store",
                path.display(),
                channel
            ),
            Ok(None) => format!(
                "release channel: path={} channel={} source=default",
                path.display(),
                ReleaseChannel::Stable
            ),
            Err(error) => format!(
                "release channel error: path={} error={error}",
                path.display()
            ),
        },
        ReleaseChannelCommand::Set(channel) => match save_release_channel_store(path, channel) {
            Ok(()) => format!(
                "release channel set: path={} channel={} status=saved",
                path.display(),
                channel
            ),
            Err(error) => format!(
                "release channel error: path={} error={error}",
                path.display()
            ),
        },
        ReleaseChannelCommand::Check => {
            render_release_channel_check_with_lookup(path, env!("CARGO_PKG_VERSION"), |channel| {
                resolve_latest_channel_release(channel, RELEASE_LOOKUP_URL)
            })
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
                    format!(
                        "release cache: path={} status=present schema_version={} entries={} fetched_at_unix_ms={} age_ms={} source_url={}",
                        cache_path.display(),
                        cache.schema_version,
                        cache.releases.len(),
                        cache.fetched_at_unix_ms,
                        age_ms,
                        cache.source_url
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
    fn unit_parse_release_channel_command_supports_show_set_and_check() {
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
            parse_release_channel_command("cache show").expect("cache show command"),
            ReleaseChannelCommand::CacheShow
        );
        assert_eq!(
            parse_release_channel_command("cache clear").expect("cache clear command"),
            ReleaseChannelCommand::CacheClear
        );

        let invalid = parse_release_channel_command("set nightly").expect_err("invalid channel");
        assert!(invalid.to_string().contains("expected stable|beta|dev"));

        let invalid = parse_release_channel_command("check now").expect_err("invalid extra arg");
        assert!(invalid.to_string().contains(RELEASE_CHANNEL_USAGE));
        let invalid =
            parse_release_channel_command("cache inspect").expect_err("invalid cache subcommand");
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
    fn functional_execute_release_channel_command_show_and_set_round_trip() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("release-channel.json");

        let initial = execute_release_channel_command("", &path);
        assert!(initial.contains("channel=stable"));
        assert!(initial.contains("source=default"));

        let set_output = execute_release_channel_command("set dev", &path);
        assert!(set_output.contains("channel=dev"));
        assert!(set_output.contains("status=saved"));

        let show = execute_release_channel_command("show", &path);
        assert!(show.contains("channel=dev"));
        assert!(show.contains("source=store"));
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
        assert!(present.contains("source_url=https://example.invalid/releases"));

        let cleared = execute_release_channel_command("cache clear", &path);
        assert!(cleared.contains("status=removed"));
        assert!(!cache_path.exists());

        let cleared_again = execute_release_channel_command("cache clear", &path);
        assert!(cleared_again.contains("status=already_missing"));
    }

    #[test]
    fn functional_render_release_channel_check_reports_update_available() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("release-channel.json");
        let output =
            render_release_channel_check_with_lookup(&path, "0.1.0", |_| Ok(Some("v0.2.0".into())));
        assert!(output.contains("channel=stable"));
        assert!(output.contains("channel_source=default"));
        assert!(output.contains("current=0.1.0"));
        assert!(output.contains("latest=v0.2.0"));
        assert!(output.contains("status=update_available"));
    }

    #[test]
    fn functional_render_release_channel_check_reports_up_to_date_when_current_is_newer() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join(".tau/release-channel.json");
        save_release_channel_store(&path, ReleaseChannel::Beta).expect("save release channel");

        let output =
            render_release_channel_check_with_lookup(&path, "0.3.0", |_| Ok(Some("v0.2.5".into())));
        assert!(output.contains("channel=beta"));
        assert!(output.contains("channel_source=store"));
        assert!(output.contains("status=up_to_date"));
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
}
