use anyhow::Result;
use std::path::Path;
use tau_release_channel::{load_release_channel_store, save_release_channel_store, ReleaseChannel};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OnboardingReleaseChannelState {
    pub channel: ReleaseChannel,
    pub source: &'static str,
    pub action: &'static str,
}

fn resolve_onboarding_release_channel_override(
    raw: Option<&str>,
) -> Result<Option<ReleaseChannel>> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    Ok(Some(trimmed.parse::<ReleaseChannel>()?))
}

pub fn ensure_onboarding_release_channel(
    release_channel_path: &Path,
    override_channel_raw: Option<&str>,
) -> Result<OnboardingReleaseChannelState> {
    let override_channel = resolve_onboarding_release_channel_override(override_channel_raw)?;
    let existing = load_release_channel_store(release_channel_path)?;

    match (override_channel, existing) {
        (Some(channel), Some(existing_channel)) if channel == existing_channel => {
            Ok(OnboardingReleaseChannelState {
                channel,
                source: "override",
                action: "unchanged",
            })
        }
        (Some(channel), Some(_)) => {
            save_release_channel_store(release_channel_path, channel)?;
            Ok(OnboardingReleaseChannelState {
                channel,
                source: "override",
                action: "updated",
            })
        }
        (Some(channel), None) => {
            save_release_channel_store(release_channel_path, channel)?;
            Ok(OnboardingReleaseChannelState {
                channel,
                source: "override",
                action: "created",
            })
        }
        (None, Some(channel)) => Ok(OnboardingReleaseChannelState {
            channel,
            source: "existing",
            action: "unchanged",
        }),
        (None, None) => {
            save_release_channel_store(release_channel_path, ReleaseChannel::Stable)?;
            Ok(OnboardingReleaseChannelState {
                channel: ReleaseChannel::Stable,
                source: "default",
                action: "created",
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ensure_onboarding_release_channel;
    use tau_release_channel::{load_release_channel_store, ReleaseChannel};
    use tempfile::tempdir;

    #[test]
    fn functional_ensure_onboarding_release_channel_defaults_to_stable_when_missing() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join(".tau/release-channel.json");
        let state =
            ensure_onboarding_release_channel(&path, None).expect("ensure default release channel");
        assert_eq!(state.channel, ReleaseChannel::Stable);
        assert_eq!(state.source, "default");
        assert_eq!(state.action, "created");
        let stored = load_release_channel_store(&path)
            .expect("load stored release channel")
            .expect("stored channel");
        assert_eq!(stored, ReleaseChannel::Stable);
    }

    #[test]
    fn functional_ensure_onboarding_release_channel_override_updates_existing_value() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join(".tau/release-channel.json");
        let first =
            ensure_onboarding_release_channel(&path, Some("beta")).expect("create beta release");
        assert_eq!(first.channel, ReleaseChannel::Beta);
        assert_eq!(first.action, "created");

        let second =
            ensure_onboarding_release_channel(&path, Some("dev")).expect("update to dev release");
        assert_eq!(second.channel, ReleaseChannel::Dev);
        assert_eq!(second.source, "override");
        assert_eq!(second.action, "updated");
    }

    #[test]
    fn regression_ensure_onboarding_release_channel_rejects_invalid_override() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join(".tau/release-channel.json");
        let error = ensure_onboarding_release_channel(&path, Some("nightly"))
            .expect_err("invalid override should fail");
        assert!(error.to_string().contains("expected stable|beta|dev"));
    }
}
