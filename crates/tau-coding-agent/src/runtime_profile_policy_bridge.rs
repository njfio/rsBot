//! Profile-policy bridge for runtime heartbeat hot-reload.
//!
//! This module projects active profile store updates into the existing
//! `<state-path>.policy.toml` heartbeat hot-reload channel.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use tau_cli::Cli;
use tau_onboarding::profile_store::{default_profile_store_path, load_profile_store};
use tau_onboarding::startup_config::ProfileDefaults;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing::{info, warn};

use tau_core::write_text_atomic;

const DEFAULT_PROFILE_NAME: &str = "default";
const PROFILE_BRIDGE_POLL_INTERVAL_MS: u64 = 1_000;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProfileStoreFingerprint {
    exists: bool,
    len: Option<u64>,
    modified: Option<SystemTime>,
}

impl ProfileStoreFingerprint {
    fn read(path: &Path) -> Self {
        match std::fs::metadata(path) {
            Ok(metadata) => Self {
                exists: true,
                len: Some(metadata.len()),
                modified: metadata.modified().ok(),
            },
            Err(_) => Self {
                exists: false,
                len: None,
                modified: None,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProfilePolicyBridgeOutcome {
    Applied {
        interval_ms: u64,
        policy_path: PathBuf,
    },
    NoChange {
        interval_ms: u64,
    },
    Invalid {
        reason: String,
    },
    MissingProfile {
        profile: String,
    },
}

impl ProfilePolicyBridgeOutcome {
    fn reason_code(&self) -> &'static str {
        match self {
            Self::Applied { .. } => "profile_policy_bridge_applied",
            Self::NoChange { .. } => "profile_policy_bridge_no_change",
            Self::Invalid { .. } => "profile_policy_bridge_invalid",
            Self::MissingProfile { .. } => "profile_policy_bridge_missing_profile",
        }
    }
}

#[derive(Debug)]
struct RuntimeHeartbeatProfilePolicyBridge {
    profile_store_path: PathBuf,
    profile_name: String,
    heartbeat_state_path: PathBuf,
    last_applied_interval_ms: u64,
    last_fingerprint: Option<ProfileStoreFingerprint>,
}

impl RuntimeHeartbeatProfilePolicyBridge {
    fn new(
        profile_store_path: PathBuf,
        profile_name: String,
        heartbeat_state_path: PathBuf,
        initial_interval_ms: u64,
    ) -> Self {
        Self {
            profile_store_path,
            profile_name,
            heartbeat_state_path,
            last_applied_interval_ms: initial_interval_ms.max(1),
            last_fingerprint: None,
        }
    }

    fn evaluate_if_changed(&mut self, force: bool) -> ProfilePolicyBridgeOutcome {
        let current_fingerprint = ProfileStoreFingerprint::read(&self.profile_store_path);
        let changed = self
            .last_fingerprint
            .as_ref()
            .map(|previous| previous != &current_fingerprint)
            .unwrap_or(true);
        self.last_fingerprint = Some(current_fingerprint);
        if !force && !changed {
            return ProfilePolicyBridgeOutcome::NoChange {
                interval_ms: self.last_applied_interval_ms,
            };
        }
        self.evaluate_active_profile()
    }

    fn evaluate_active_profile(&mut self) -> ProfilePolicyBridgeOutcome {
        let profiles = match load_profile_store(&self.profile_store_path) {
            Ok(profiles) => profiles,
            Err(error) => {
                return ProfilePolicyBridgeOutcome::Invalid {
                    reason: format!(
                        "profile_store_load_failed: path={} error={error}",
                        self.profile_store_path.display()
                    ),
                };
            }
        };
        let Some(profile_defaults) = profiles.get(&self.profile_name) else {
            return ProfilePolicyBridgeOutcome::MissingProfile {
                profile: self.profile_name.clone(),
            };
        };
        self.apply_profile_policy(profile_defaults)
    }

    fn apply_profile_policy(
        &mut self,
        profile_defaults: &ProfileDefaults,
    ) -> ProfilePolicyBridgeOutcome {
        let interval_ms = profile_defaults.policy.runtime_heartbeat_interval_ms;
        if interval_ms == 0 {
            return ProfilePolicyBridgeOutcome::Invalid {
                reason: "profile_interval_invalid_zero".to_string(),
            };
        }
        if interval_ms == self.last_applied_interval_ms {
            return ProfilePolicyBridgeOutcome::NoChange { interval_ms };
        }
        match write_runtime_heartbeat_policy_file(&self.heartbeat_state_path, interval_ms) {
            Ok(policy_path) => {
                self.last_applied_interval_ms = interval_ms;
                ProfilePolicyBridgeOutcome::Applied {
                    interval_ms,
                    policy_path,
                }
            }
            Err(error) => ProfilePolicyBridgeOutcome::Invalid {
                reason: format!(
                    "policy_write_failed: state_path={} error={error}",
                    self.heartbeat_state_path.display()
                ),
            },
        }
    }
}

fn runtime_heartbeat_policy_path(state_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.policy.toml", state_path.display()))
}

fn write_runtime_heartbeat_policy_file(state_path: &Path, interval_ms: u64) -> Result<PathBuf> {
    let policy_path = runtime_heartbeat_policy_path(state_path);
    if let Some(parent) = policy_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create profile policy dir {}", parent.display()))?;
    }
    write_text_atomic(
        &policy_path,
        format!("interval_ms = {interval_ms}\n").as_str(),
    )
    .with_context(|| format!("failed to persist profile policy {}", policy_path.display()))?;
    Ok(policy_path)
}

#[derive(Debug)]
pub(crate) struct RuntimeHeartbeatProfilePolicyBridgeHandle {
    shutdown_tx: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

impl RuntimeHeartbeatProfilePolicyBridgeHandle {
    fn disabled() -> Self {
        Self {
            shutdown_tx: None,
            task: None,
        }
    }

    pub(crate) async fn shutdown(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

pub(crate) fn start_runtime_heartbeat_profile_policy_bridge(
    cli: &Cli,
) -> Result<RuntimeHeartbeatProfilePolicyBridgeHandle> {
    if !cli.runtime_heartbeat_enabled {
        return Ok(RuntimeHeartbeatProfilePolicyBridgeHandle::disabled());
    }
    let profile_store_path = default_profile_store_path()?;
    let profile_name = DEFAULT_PROFILE_NAME.to_string();
    let state_path = cli.runtime_heartbeat_state_path.clone();
    let initial_interval_ms = cli.runtime_heartbeat_interval_ms.max(1);

    let mut bridge = RuntimeHeartbeatProfilePolicyBridge::new(
        profile_store_path,
        profile_name,
        state_path,
        initial_interval_ms,
    );
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(Duration::from_millis(PROFILE_BRIDGE_POLL_INTERVAL_MS));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let initial = bridge.evaluate_if_changed(true);
        emit_bridge_outcome(&initial);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let outcome = bridge.evaluate_if_changed(false);
                    if !matches!(outcome, ProfilePolicyBridgeOutcome::NoChange { .. }) {
                        emit_bridge_outcome(&outcome);
                    }
                }
                _ = &mut shutdown_rx => {
                    break;
                }
            }
        }
    });
    Ok(RuntimeHeartbeatProfilePolicyBridgeHandle {
        shutdown_tx: Some(shutdown_tx),
        task: Some(task),
    })
}

fn emit_bridge_outcome(outcome: &ProfilePolicyBridgeOutcome) {
    match outcome {
        ProfilePolicyBridgeOutcome::Applied {
            interval_ms,
            policy_path,
        } => info!(
            reason_code = outcome.reason_code(),
            interval_ms = *interval_ms,
            policy_path = %policy_path.display(),
            "runtime heartbeat profile-policy bridge applied interval update"
        ),
        ProfilePolicyBridgeOutcome::NoChange { interval_ms } => info!(
            reason_code = outcome.reason_code(),
            interval_ms = *interval_ms,
            "runtime heartbeat profile-policy bridge observed no effective change"
        ),
        ProfilePolicyBridgeOutcome::Invalid { reason } => warn!(
            reason_code = outcome.reason_code(),
            diagnostic = %reason,
            "runtime heartbeat profile-policy bridge ignored invalid profile payload"
        ),
        ProfilePolicyBridgeOutcome::MissingProfile { profile } => warn!(
            reason_code = outcome.reason_code(),
            profile = %profile,
            "runtime heartbeat profile-policy bridge missing active profile"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        runtime_heartbeat_policy_path, start_runtime_heartbeat_profile_policy_bridge,
        ProfilePolicyBridgeOutcome, RuntimeHeartbeatProfilePolicyBridge,
    };
    use crate::tests::test_cli;
    use std::collections::BTreeMap;
    use std::time::Duration;
    use tau_onboarding::profile_store::save_profile_store;
    use tempfile::tempdir;

    fn sample_profile(interval_ms: u64) -> tau_onboarding::startup_config::ProfileDefaults {
        let mut cli = test_cli();
        cli.runtime_heartbeat_interval_ms = interval_ms.max(1);
        crate::build_profile_defaults(&cli)
    }

    fn write_profile_store(
        path: &std::path::Path,
        profile_name: &str,
        interval_ms: u64,
    ) -> Result<(), anyhow::Error> {
        let mut profiles = BTreeMap::new();
        profiles.insert(profile_name.to_string(), sample_profile(interval_ms));
        save_profile_store(path, &profiles)
    }

    #[test]
    fn integration_spec_2541_c01_profile_policy_bridge_applies_updated_interval_policy() {
        let temp = tempdir().expect("tempdir");
        let profile_path = temp.path().join(".tau/profiles.json");
        let state_path = temp.path().join(".tau/runtime-heartbeat/state.json");
        write_profile_store(&profile_path, "default", 1_200).expect("write profile store");

        let mut bridge = RuntimeHeartbeatProfilePolicyBridge::new(
            profile_path,
            "default".to_string(),
            state_path.clone(),
            5_000,
        );
        let outcome = bridge.evaluate_if_changed(true);
        match outcome {
            ProfilePolicyBridgeOutcome::Applied { interval_ms, .. } => {
                assert_eq!(interval_ms, 1_200);
            }
            other => panic!("expected applied outcome, got {other:?}"),
        }
        let policy_path = runtime_heartbeat_policy_path(&state_path);
        let raw = std::fs::read_to_string(&policy_path).expect("read policy path");
        assert!(
            raw.contains("interval_ms = 1200"),
            "policy should contain updated interval"
        );
    }

    #[test]
    fn regression_spec_2541_c02_profile_policy_bridge_no_change_does_not_rewrite_policy_file() {
        let temp = tempdir().expect("tempdir");
        let profile_path = temp.path().join(".tau/profiles.json");
        let state_path = temp.path().join(".tau/runtime-heartbeat/state.json");
        write_profile_store(&profile_path, "default", 5_000).expect("write profile store");

        let mut bridge = RuntimeHeartbeatProfilePolicyBridge::new(
            profile_path,
            "default".to_string(),
            state_path.clone(),
            5_000,
        );
        let outcome = bridge.evaluate_if_changed(true);
        match outcome {
            ProfilePolicyBridgeOutcome::NoChange { interval_ms } => assert_eq!(interval_ms, 5_000),
            other => panic!("expected no-change outcome, got {other:?}"),
        }
        assert!(
            !runtime_heartbeat_policy_path(&state_path).exists(),
            "no-op should not emit policy write"
        );
    }

    #[test]
    fn regression_spec_2541_c03_profile_policy_bridge_invalid_profile_store_preserves_last_interval(
    ) {
        let temp = tempdir().expect("tempdir");
        let profile_path = temp.path().join(".tau/profiles.json");
        let state_path = temp.path().join(".tau/runtime-heartbeat/state.json");
        std::fs::create_dir_all(profile_path.parent().expect("parent"))
            .expect("create profile dir");
        std::fs::write(&profile_path, "{invalid").expect("write invalid profile payload");

        let mut bridge = RuntimeHeartbeatProfilePolicyBridge::new(
            profile_path,
            "default".to_string(),
            state_path.clone(),
            5_000,
        );
        let outcome = bridge.evaluate_if_changed(true);
        match outcome {
            ProfilePolicyBridgeOutcome::Invalid { .. } => {}
            other => panic!("expected invalid outcome, got {other:?}"),
        }
        assert!(
            !runtime_heartbeat_policy_path(&state_path).exists(),
            "invalid profile payload must not write policy file"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn integration_spec_2541_c04_profile_policy_bridge_start_and_shutdown_is_clean() {
        let temp = tempdir().expect("tempdir");
        let original_cwd = std::env::current_dir().expect("resolve cwd");
        std::env::set_current_dir(temp.path()).expect("set current dir");
        let mut cli = test_cli();
        cli.runtime_heartbeat_enabled = true;
        cli.runtime_heartbeat_interval_ms = 900;
        cli.runtime_heartbeat_state_path = temp.path().join(".tau/runtime-heartbeat/state.json");
        let profile_path = temp.path().join(".tau/profiles.json");
        write_profile_store(&profile_path, "default", 900).expect("write profile store");

        let mut handle = start_runtime_heartbeat_profile_policy_bridge(&cli).expect("start bridge");
        tokio::time::sleep(Duration::from_millis(80)).await;
        handle.shutdown().await;
        std::env::set_current_dir(original_cwd).expect("restore cwd");
    }
}
