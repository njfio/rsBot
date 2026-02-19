//! Profile-policy bridge for runtime heartbeat hot-reload.
//!
//! This module projects active profile store updates into the existing
//! `<state-path>.policy.toml` heartbeat hot-reload channel.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::{Context, Result};
use arc_swap::ArcSwap;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use tau_cli::Cli;
use tau_onboarding::profile_store::{default_profile_store_path, load_profile_store};
use tau_onboarding::startup_config::ProfileDefaults;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::{info, warn};

use tau_core::write_text_atomic;

const DEFAULT_PROFILE_NAME: &str = "default";

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeHeartbeatActivePolicyConfig {
    interval_ms: u64,
}

impl RuntimeHeartbeatActivePolicyConfig {
    fn new(interval_ms: u64) -> Self {
        Self {
            interval_ms: interval_ms.max(1),
        }
    }
}

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
enum ProfilePolicyBridgeWatcherEvent {
    ProfileChanged,
    WatchError(String),
}

fn start_profile_store_watcher(
    profile_store_path: &Path,
) -> (
    Option<RecommendedWatcher>,
    Option<mpsc::UnboundedReceiver<ProfilePolicyBridgeWatcherEvent>>,
    Vec<String>,
) {
    let mut diagnostics = Vec::new();
    let Some(profile_parent) = profile_store_path.parent().map(Path::to_path_buf) else {
        diagnostics.push(format!(
            "profile_policy_watch_parent_missing: path={}",
            profile_store_path.display()
        ));
        return (None, None, diagnostics);
    };

    let (watch_tx, watch_rx) = mpsc::unbounded_channel::<ProfilePolicyBridgeWatcherEvent>();
    let watched_profile_path = profile_store_path.to_path_buf();
    let watched_profile_parent = profile_parent.clone();
    let watched_profile_name = profile_store_path
        .file_name()
        .map(std::ffi::OsStr::to_os_string);
    let watcher = notify::recommended_watcher(move |result: notify::Result<Event>| match result {
        Ok(event) => {
            let matches_profile = event.paths.is_empty()
                || event.paths.iter().any(|candidate_path| {
                    candidate_path == &watched_profile_path
                        || candidate_path == &watched_profile_parent
                        || watched_profile_name
                            .as_ref()
                            .is_some_and(|name| candidate_path.file_name() == Some(name))
                });
            if matches_profile {
                let _ = watch_tx.send(ProfilePolicyBridgeWatcherEvent::ProfileChanged);
            }
        }
        Err(error) => {
            let _ = watch_tx.send(ProfilePolicyBridgeWatcherEvent::WatchError(
                error.to_string(),
            ));
        }
    });

    let mut watcher = match watcher {
        Ok(watcher) => watcher,
        Err(error) => {
            diagnostics.push(format!(
                "profile_policy_watch_init_failed: path={} error={error}",
                profile_store_path.display()
            ));
            return (None, None, diagnostics);
        }
    };

    if let Err(error) = watcher.watch(profile_parent.as_path(), RecursiveMode::NonRecursive) {
        diagnostics.push(format!(
            "profile_policy_watch_start_failed: path={} parent={} error={error}",
            profile_store_path.display(),
            profile_parent.display()
        ));
        return (None, None, diagnostics);
    }

    (Some(watcher), Some(watch_rx), diagnostics)
}

#[derive(Debug)]
struct RuntimeHeartbeatProfilePolicyBridge {
    profile_store_path: PathBuf,
    profile_name: String,
    heartbeat_state_path: PathBuf,
    active_policy_config: Arc<ArcSwap<RuntimeHeartbeatActivePolicyConfig>>,
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
            active_policy_config: Arc::new(ArcSwap::from_pointee(
                RuntimeHeartbeatActivePolicyConfig::new(initial_interval_ms),
            )),
            last_fingerprint: None,
        }
    }

    fn active_interval_ms(&self) -> u64 {
        self.active_policy_config.load().interval_ms
    }

    #[cfg(test)]
    fn set_active_interval_ms_for_test(&self, interval_ms: u64) {
        self.active_policy_config
            .store(Arc::new(RuntimeHeartbeatActivePolicyConfig::new(
                interval_ms,
            )));
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
                interval_ms: self.active_interval_ms(),
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
        if interval_ms == self.active_interval_ms() {
            return ProfilePolicyBridgeOutcome::NoChange { interval_ms };
        }
        match write_runtime_heartbeat_policy_file(&self.heartbeat_state_path, interval_ms) {
            Ok(policy_path) => {
                self.active_policy_config
                    .store(Arc::new(RuntimeHeartbeatActivePolicyConfig::new(
                        interval_ms,
                    )));
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
    let profile_store_path = profile_store_path_for_runtime_heartbeat(cli)?;
    let profile_name = DEFAULT_PROFILE_NAME.to_string();
    let state_path = cli.runtime_heartbeat_state_path.clone();
    let initial_interval_ms = cli.runtime_heartbeat_interval_ms.max(1);

    let mut bridge = RuntimeHeartbeatProfilePolicyBridge::new(
        profile_store_path,
        profile_name,
        state_path,
        initial_interval_ms,
    );
    let watch_path = bridge.profile_store_path.clone();
    let (watcher, mut watch_rx, watcher_diagnostics) = start_profile_store_watcher(&watch_path);
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let _watcher = watcher;
        let initial = bridge.evaluate_if_changed(true);
        emit_bridge_outcome(&initial);
        for diagnostic in watcher_diagnostics {
            emit_bridge_outcome(&ProfilePolicyBridgeOutcome::Invalid { reason: diagnostic });
        }

        loop {
            tokio::select! {
                event = async {
                    match watch_rx.as_mut() {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending::<Option<ProfilePolicyBridgeWatcherEvent>>().await,
                    }
                } => {
                    match event {
                        Some(ProfilePolicyBridgeWatcherEvent::ProfileChanged) => {
                            // A watcher event is itself a change signal; force re-evaluation to
                            // avoid missing same-size rapid rewrites that can alias fingerprint
                            // metadata.
                            let outcome = bridge.evaluate_if_changed(true);
                            emit_bridge_outcome(&outcome);
                        }
                        Some(ProfilePolicyBridgeWatcherEvent::WatchError(error)) => {
                            emit_bridge_outcome(&ProfilePolicyBridgeOutcome::Invalid {
                                reason: format!(
                                    "profile_policy_watch_error: path={} error={error}",
                                    bridge.profile_store_path.display()
                                ),
                            });
                        }
                        None => {
                            watch_rx = None;
                            emit_bridge_outcome(&ProfilePolicyBridgeOutcome::Invalid {
                                reason: format!(
                                    "profile_policy_watch_disconnected: path={}",
                                    bridge.profile_store_path.display()
                                ),
                            });
                        }
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

fn profile_store_path_for_runtime_heartbeat(cli: &Cli) -> Result<PathBuf> {
    let derived_path = cli
        .runtime_heartbeat_state_path
        .parent()
        .and_then(Path::parent)
        .filter(|candidate| candidate.file_name().is_some_and(|name| name == ".tau"))
        .map(|tau_root| tau_root.join("profiles.json"));
    if let Some(path) = derived_path {
        return Ok(path);
    }
    default_profile_store_path()
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
        emit_bridge_outcome, profile_store_path_for_runtime_heartbeat,
        runtime_heartbeat_policy_path, start_runtime_heartbeat_profile_policy_bridge,
        ProfilePolicyBridgeOutcome, RuntimeHeartbeatProfilePolicyBridge,
        RuntimeHeartbeatProfilePolicyBridgeHandle,
    };
    use crate::tests::test_cli;
    use std::collections::BTreeMap;
    use std::io::{self, Write};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tau_onboarding::profile_store::save_profile_store;
    use tempfile::tempdir;
    use tokio::sync::oneshot;
    use tracing_subscriber::fmt::MakeWriter;

    #[derive(Clone, Default)]
    struct SharedLogBuffer {
        inner: Arc<Mutex<Vec<u8>>>,
    }

    struct SharedLogWriter {
        inner: Arc<Mutex<Vec<u8>>>,
    }

    impl<'a> MakeWriter<'a> for SharedLogBuffer {
        type Writer = SharedLogWriter;

        fn make_writer(&'a self) -> Self::Writer {
            SharedLogWriter {
                inner: Arc::clone(&self.inner),
            }
        }
    }

    impl Write for SharedLogWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let mut locked = self.inner.lock().expect("log buffer lock");
            locked.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl SharedLogBuffer {
        fn contents(&self) -> String {
            let bytes = self.inner.lock().expect("log buffer lock").clone();
            String::from_utf8(bytes).expect("valid utf8 logs")
        }
    }

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

    async fn wait_for_policy_interval(
        policy_path: &std::path::Path,
        expected_interval_ms: u64,
        timeout_ms: u64,
    ) -> bool {
        let expected = format!("interval_ms = {expected_interval_ms}");
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            if let Ok(raw) = std::fs::read_to_string(policy_path) {
                if raw.contains(&expected) {
                    return true;
                }
            }
            if tokio::time::Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
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

    #[test]
    fn regression_spec_2541_c05_profile_policy_bridge_detects_non_forced_profile_updates() {
        let temp = tempdir().expect("tempdir");
        let profile_path = temp.path().join(".tau/profiles.json");
        let state_path = temp.path().join(".tau/runtime-heartbeat/state.json");
        write_profile_store(&profile_path, "default", 1_200).expect("write initial profile store");

        let mut bridge = RuntimeHeartbeatProfilePolicyBridge::new(
            profile_path.clone(),
            "default".to_string(),
            state_path,
            5_000,
        );
        let initial = bridge.evaluate_if_changed(true);
        assert!(
            matches!(
                initial,
                ProfilePolicyBridgeOutcome::Applied {
                    interval_ms: 1_200,
                    ..
                }
            ),
            "initial force evaluation should apply 1200 interval"
        );

        std::thread::sleep(Duration::from_millis(5));
        write_profile_store(&profile_path, "default", 12_000).expect("write updated profile store");
        let updated = bridge.evaluate_if_changed(false);
        assert!(
            matches!(
                updated,
                ProfilePolicyBridgeOutcome::Applied {
                    interval_ms: 12_000,
                    ..
                }
            ),
            "non-forced evaluation should detect fingerprint change and apply update"
        );
    }

    #[test]
    fn regression_spec_2541_c06_profile_policy_bridge_force_reload_bypasses_fingerprint_noop() {
        let temp = tempdir().expect("tempdir");
        let profile_path = temp.path().join(".tau/profiles.json");
        let state_path = temp.path().join(".tau/runtime-heartbeat/state.json");
        write_profile_store(&profile_path, "default", 1_200).expect("write profile store");

        let mut bridge = RuntimeHeartbeatProfilePolicyBridge::new(
            profile_path,
            "default".to_string(),
            state_path,
            1_200,
        );
        let first = bridge.evaluate_if_changed(true);
        assert!(
            matches!(
                first,
                ProfilePolicyBridgeOutcome::NoChange { interval_ms: 1_200 }
            ),
            "first force evaluation should initialize fingerprint and report no change"
        );

        bridge.set_active_interval_ms_for_test(900);
        let forced = bridge.evaluate_if_changed(true);
        assert!(
            matches!(
                forced,
                ProfilePolicyBridgeOutcome::Applied {
                    interval_ms: 1_200,
                    ..
                }
            ),
            "forced evaluation must reload profile even when fingerprint is unchanged"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn integration_spec_2541_c04_profile_policy_bridge_start_and_shutdown_is_clean() {
        let temp = tempdir().expect("tempdir");
        let mut cli = test_cli();
        cli.runtime_heartbeat_enabled = true;
        cli.runtime_heartbeat_interval_ms = 900;
        cli.runtime_heartbeat_state_path = temp.path().join(".tau/runtime-heartbeat/state.json");
        let profile_path = temp.path().join(".tau/profiles.json");
        write_profile_store(&profile_path, "default", 900).expect("write profile store");

        let mut handle = start_runtime_heartbeat_profile_policy_bridge(&cli).expect("start bridge");
        tokio::time::sleep(Duration::from_millis(80)).await;
        handle.shutdown().await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn spec_2597_c01_profile_policy_bridge_notify_events_trigger_reload() {
        let temp = tempdir().expect("tempdir");
        let mut cli = test_cli();
        cli.runtime_heartbeat_enabled = true;
        cli.runtime_heartbeat_interval_ms = 900;
        cli.runtime_heartbeat_state_path = temp.path().join(".tau/runtime-heartbeat/state.json");
        let profile_path = temp.path().join(".tau/profiles.json");
        write_profile_store(&profile_path, "default", 900).expect("write profile store");

        let mut handle = start_runtime_heartbeat_profile_policy_bridge(&cli).expect("start bridge");
        tokio::time::sleep(Duration::from_millis(120)).await;

        write_profile_store(&profile_path, "default", 1_200).expect("write updated profile store");
        let policy_path = runtime_heartbeat_policy_path(&cli.runtime_heartbeat_state_path);
        assert!(
            wait_for_policy_interval(&policy_path, 1_200, 2_000).await,
            "profile change should trigger reload promptly through notify watcher"
        );

        handle.shutdown().await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn spec_2597_c02_profile_policy_bridge_arcswap_updates_on_valid_change() {
        let temp = tempdir().expect("tempdir");
        let mut cli = test_cli();
        cli.runtime_heartbeat_enabled = true;
        cli.runtime_heartbeat_interval_ms = 700;
        cli.runtime_heartbeat_state_path = temp.path().join(".tau/runtime-heartbeat/state.json");
        let profile_path = temp.path().join(".tau/profiles.json");
        write_profile_store(&profile_path, "default", 900).expect("write initial profile store");

        let mut handle = start_runtime_heartbeat_profile_policy_bridge(&cli).expect("start bridge");
        let policy_path = runtime_heartbeat_policy_path(&cli.runtime_heartbeat_state_path);
        assert!(
            wait_for_policy_interval(&policy_path, 900, 2_000).await,
            "initial evaluation should apply profile interval"
        );

        write_profile_store(&profile_path, "default", 1_200).expect("write updated profile store");
        tokio::time::sleep(Duration::from_millis(120)).await;
        write_profile_store(&profile_path, "default", 1_300).expect("write second profile store");
        assert!(
            wait_for_policy_interval(&policy_path, 1_300, 2_500).await,
            "active config should swap atomically to latest valid update"
        );

        handle.shutdown().await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn regression_2597_c03_profile_policy_bridge_invalid_change_preserves_last_good_active_config(
    ) {
        let temp = tempdir().expect("tempdir");
        let mut cli = test_cli();
        cli.runtime_heartbeat_enabled = true;
        cli.runtime_heartbeat_interval_ms = 700;
        cli.runtime_heartbeat_state_path = temp.path().join(".tau/runtime-heartbeat/state.json");
        let profile_path = temp.path().join(".tau/profiles.json");
        write_profile_store(&profile_path, "default", 1_200).expect("write initial profile store");

        let mut handle = start_runtime_heartbeat_profile_policy_bridge(&cli).expect("start bridge");
        let policy_path = runtime_heartbeat_policy_path(&cli.runtime_heartbeat_state_path);
        assert!(
            wait_for_policy_interval(&policy_path, 1_200, 300).await,
            "initial profile interval should apply"
        );

        std::fs::write(&profile_path, "{invalid").expect("write invalid profile payload");
        tokio::time::sleep(Duration::from_millis(400)).await;

        let policy_raw = std::fs::read_to_string(&policy_path).expect("read policy after invalid");
        assert!(
            policy_raw.contains("interval_ms = 1200"),
            "invalid profile payload must preserve last-known-good active config"
        );

        handle.shutdown().await;
    }

    #[test]
    fn regression_2597_c04_profile_policy_bridge_emits_stable_reload_diagnostics() {
        let temp = tempdir().expect("tempdir");
        let state_path = temp.path().join(".tau/runtime-heartbeat/state.json");
        let profile_path = temp.path().join(".tau/profiles.json");
        write_profile_store(&profile_path, "default", 900).expect("write initial profile store");

        let mut bridge = RuntimeHeartbeatProfilePolicyBridge::new(
            profile_path.clone(),
            "default".to_string(),
            state_path,
            700,
        );
        let applied = bridge.evaluate_if_changed(true);
        std::thread::sleep(Duration::from_millis(5));
        write_profile_store(&profile_path, "default", 900).expect("rewrite no-change profile");
        let no_change = bridge.evaluate_if_changed(false);
        std::thread::sleep(Duration::from_millis(5));
        std::fs::write(&profile_path, "{invalid").expect("write invalid profile payload");
        let invalid = bridge.evaluate_if_changed(false);

        assert!(
            matches!(
                applied,
                ProfilePolicyBridgeOutcome::Applied {
                    interval_ms: 900,
                    ..
                }
            ),
            "initial evaluation should apply profile interval"
        );
        assert!(
            matches!(
                no_change,
                ProfilePolicyBridgeOutcome::NoChange { interval_ms: 900 }
            ),
            "rewriting equivalent interval should produce no-change outcome"
        );
        assert!(
            matches!(invalid, ProfilePolicyBridgeOutcome::Invalid { .. }),
            "invalid profile payload should produce invalid outcome"
        );
        assert_eq!(applied.reason_code(), "profile_policy_bridge_applied");
        assert_eq!(no_change.reason_code(), "profile_policy_bridge_no_change");
        assert_eq!(invalid.reason_code(), "profile_policy_bridge_invalid");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn regression_spec_2541_c07_profile_policy_bridge_start_enabled_spawns_active_handle() {
        let temp = tempdir().expect("tempdir");
        let mut cli = test_cli();
        cli.runtime_heartbeat_enabled = true;
        cli.runtime_heartbeat_interval_ms = 900;
        cli.runtime_heartbeat_state_path = temp.path().join(".tau/runtime-heartbeat/state.json");
        let profile_path = temp.path().join(".tau/profiles.json");
        write_profile_store(&profile_path, "default", 1_200).expect("write profile store");

        let mut handle = start_runtime_heartbeat_profile_policy_bridge(&cli).expect("start bridge");
        assert!(
            handle.shutdown_tx.is_some(),
            "enabled bridge should expose shutdown channel"
        );
        assert!(
            handle.task.is_some(),
            "enabled bridge should spawn background task"
        );
        tokio::time::sleep(Duration::from_millis(80)).await;

        let policy_path = runtime_heartbeat_policy_path(&cli.runtime_heartbeat_state_path);
        let policy_raw = std::fs::read_to_string(&policy_path).expect("read policy file");
        assert!(
            policy_raw.contains("interval_ms = 1200"),
            "initial bridge evaluation should write profile interval policy"
        );

        handle.shutdown().await;
        assert!(
            handle.shutdown_tx.is_none(),
            "shutdown should consume shutdown channel"
        );
        assert!(
            handle.task.is_none(),
            "shutdown should await and clear background task"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn regression_spec_2541_c08_profile_policy_bridge_handle_shutdown_stops_task() {
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            let _ = (&mut shutdown_rx).await;
        });
        let mut handle = RuntimeHeartbeatProfilePolicyBridgeHandle {
            shutdown_tx: Some(shutdown_tx),
            task: Some(task),
        };

        handle.shutdown().await;
        assert!(
            handle.shutdown_tx.is_none(),
            "shutdown should clear shutdown sender"
        );
        assert!(handle.task.is_none(), "shutdown should clear joined task");
    }

    #[test]
    fn regression_spec_2541_c09_profile_policy_bridge_outcome_reason_codes_are_stable() {
        let policy_path = PathBuf::from("/tmp/runtime-heartbeat.policy.toml");
        let applied = ProfilePolicyBridgeOutcome::Applied {
            interval_ms: 1_200,
            policy_path,
        };
        let no_change = ProfilePolicyBridgeOutcome::NoChange { interval_ms: 1_200 };
        let invalid = ProfilePolicyBridgeOutcome::Invalid {
            reason: "profile_store_load_failed".to_string(),
        };
        let missing = ProfilePolicyBridgeOutcome::MissingProfile {
            profile: "default".to_string(),
        };

        assert_eq!(applied.reason_code(), "profile_policy_bridge_applied");
        assert_eq!(no_change.reason_code(), "profile_policy_bridge_no_change");
        assert_eq!(invalid.reason_code(), "profile_policy_bridge_invalid");
        assert_eq!(
            missing.reason_code(),
            "profile_policy_bridge_missing_profile"
        );
    }

    #[test]
    fn regression_spec_2541_c10_emit_bridge_outcome_logs_reason_code_and_diagnostic() {
        let logs = SharedLogBuffer::default();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .without_time()
            .with_target(false)
            .with_writer(logs.clone())
            .finish();

        let outcome = ProfilePolicyBridgeOutcome::Invalid {
            reason: "profile_store_load_failed".to_string(),
        };
        tracing::subscriber::with_default(subscriber, || {
            emit_bridge_outcome(&outcome);
        });

        let rendered = logs.contents();
        assert!(
            rendered.contains("profile_policy_bridge_invalid"),
            "logs should include stable reason code"
        );
        assert!(
            rendered.contains("profile_store_load_failed"),
            "logs should include diagnostic payload"
        );
    }

    #[test]
    fn regression_profile_store_path_for_runtime_heartbeat_prefers_tau_root_relative_state_path() {
        let mut cli = test_cli();
        cli.runtime_heartbeat_state_path =
            PathBuf::from("/tmp/tau-test/.tau/runtime-heartbeat/state.json");
        let resolved =
            profile_store_path_for_runtime_heartbeat(&cli).expect("resolve derived profile path");
        assert_eq!(
            resolved,
            PathBuf::from("/tmp/tau-test/.tau/profiles.json"),
            "derived runtime heartbeat state path should map to sibling profile store"
        );
    }
}
