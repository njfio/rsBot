use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use tau_cli::{Cli, CliDaemonProfile};
use tau_core::{current_unix_timestamp_ms, write_text_atomic};

const TAU_DAEMON_STATE_SCHEMA_VERSION: u32 = 1;
const TAU_DAEMON_SERVICE_LABEL: &str = "io.tau.coding-agent";
const DAEMON_STATE_FILE_NAME: &str = "state.json";
const DAEMON_PID_FILE_NAME: &str = "daemon.pid";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TauDaemonConfig {
    pub state_dir: PathBuf,
    pub profile: CliDaemonProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TauDaemonStatusReport {
    pub schema_version: u32,
    pub state_path: String,
    pub installed: bool,
    pub running: bool,
    pub profile: String,
    pub service_label: String,
    pub service_file_path: String,
    pub service_file_exists: bool,
    pub pid_file_path: String,
    pub pid_file_exists: bool,
    pub pid: Option<u32>,
    pub host_os: String,
    pub profile_supported_on_host: bool,
    pub executable_path: String,
    pub executable_exists: bool,
    pub state_dir_exists: bool,
    pub state_dir_writable: bool,
    pub last_install_unix_ms: Option<u64>,
    pub last_start_unix_ms: Option<u64>,
    pub last_stop_unix_ms: Option<u64>,
    pub last_stop_reason: Option<String>,
    pub start_attempts: u64,
    pub stop_attempts: u64,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TauDaemonLifecycleState {
    schema_version: u32,
    installed: bool,
    running: bool,
    profile: String,
    service_label: String,
    service_file_path: String,
    pid: Option<u32>,
    last_install_unix_ms: Option<u64>,
    last_start_unix_ms: Option<u64>,
    last_stop_unix_ms: Option<u64>,
    last_stop_reason: Option<String>,
    start_attempts: u64,
    stop_attempts: u64,
}

impl TauDaemonLifecycleState {
    fn default_with_service_path(profile: CliDaemonProfile, service_file_path: &Path) -> Self {
        Self {
            schema_version: TAU_DAEMON_STATE_SCHEMA_VERSION,
            installed: false,
            running: false,
            profile: profile.as_str().to_string(),
            service_label: TAU_DAEMON_SERVICE_LABEL.to_string(),
            service_file_path: service_file_path.display().to_string(),
            pid: None,
            last_install_unix_ms: None,
            last_start_unix_ms: None,
            last_stop_unix_ms: None,
            last_stop_reason: None,
            start_attempts: 0,
            stop_attempts: 0,
        }
    }
}

pub fn resolve_tau_daemon_profile(profile: CliDaemonProfile) -> CliDaemonProfile {
    if !matches!(profile, CliDaemonProfile::Auto) {
        return profile;
    }
    if cfg!(target_os = "macos") {
        CliDaemonProfile::Launchd
    } else {
        CliDaemonProfile::SystemdUser
    }
}

pub fn tau_daemon_mode_requested(cli: &Cli) -> bool {
    cli.daemon_install
        || cli.daemon_uninstall
        || cli.daemon_start
        || cli.daemon_stop
        || cli.daemon_status
}

pub fn install_tau_daemon(config: &TauDaemonConfig) -> Result<TauDaemonStatusReport> {
    let profile = resolve_tau_daemon_profile(config.profile);
    let service_file_path = daemon_service_file_path(&config.state_dir, profile);
    let state_path = daemon_state_path(&config.state_dir);

    std::fs::create_dir_all(&config.state_dir)
        .with_context(|| format!("failed to create {}", config.state_dir.display()))?;
    if let Some(parent) = service_file_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let executable_path = resolve_executable_path();
    let unit_content = match profile {
        CliDaemonProfile::Launchd => render_launchd_plist(
            TAU_DAEMON_SERVICE_LABEL,
            executable_path.as_path(),
            &config.state_dir,
        ),
        CliDaemonProfile::SystemdUser => render_systemd_user_unit(
            TAU_DAEMON_SERVICE_LABEL,
            executable_path.as_path(),
            &config.state_dir,
        ),
        CliDaemonProfile::Auto => unreachable!("auto profile should be resolved"),
    };
    write_text_atomic(&service_file_path, unit_content.as_str()).with_context(|| {
        format!(
            "failed to write daemon service file {}",
            service_file_path.display()
        )
    })?;

    let mut state = load_tau_daemon_state(&state_path, profile)?;
    state.installed = true;
    state.running = false;
    state.profile = profile.as_str().to_string();
    state.service_file_path = service_file_path.display().to_string();
    state.last_install_unix_ms = Some(current_unix_timestamp_ms());
    state.pid = None;
    state.last_stop_reason = None;
    save_tau_daemon_state(&state_path, &state)?;

    inspect_tau_daemon(config)
}

pub fn uninstall_tau_daemon(config: &TauDaemonConfig) -> Result<TauDaemonStatusReport> {
    let profile = resolve_tau_daemon_profile(config.profile);
    let state_path = daemon_state_path(&config.state_dir);
    let mut state = load_tau_daemon_state(&state_path, profile)?;
    let service_file_path = PathBuf::from(&state.service_file_path);
    let pid_path = daemon_pid_path(&config.state_dir);

    if service_file_path.exists() {
        std::fs::remove_file(&service_file_path).with_context(|| {
            format!(
                "failed to remove daemon service file {}",
                service_file_path.display()
            )
        })?;
    }
    if pid_path.exists() {
        std::fs::remove_file(&pid_path)
            .with_context(|| format!("failed to remove daemon pid file {}", pid_path.display()))?;
    }

    state.installed = false;
    state.running = false;
    state.pid = None;
    state.last_stop_unix_ms = Some(current_unix_timestamp_ms());
    state.last_stop_reason = Some("daemon_uninstall".to_string());
    state.stop_attempts = state.stop_attempts.saturating_add(1);
    save_tau_daemon_state(&state_path, &state)?;

    inspect_tau_daemon(config)
}

pub fn start_tau_daemon(config: &TauDaemonConfig) -> Result<TauDaemonStatusReport> {
    let profile = resolve_tau_daemon_profile(config.profile);
    let state_path = daemon_state_path(&config.state_dir);
    let pid_path = daemon_pid_path(&config.state_dir);
    let mut state = load_tau_daemon_state(&state_path, profile)?;

    if !state.installed {
        bail!(
            "tau daemon is not installed in '{}'; run --daemon-install first",
            config.state_dir.display()
        );
    }

    let pid = std::process::id();
    write_text_atomic(&pid_path, format!("{pid}\n").as_str())
        .with_context(|| format!("failed to write daemon pid file {}", pid_path.display()))?;
    state.running = true;
    state.pid = Some(pid);
    state.last_start_unix_ms = Some(current_unix_timestamp_ms());
    state.last_stop_reason = None;
    state.start_attempts = state.start_attempts.saturating_add(1);
    save_tau_daemon_state(&state_path, &state)?;

    inspect_tau_daemon(config)
}

pub fn stop_tau_daemon(
    config: &TauDaemonConfig,
    reason: Option<&str>,
) -> Result<TauDaemonStatusReport> {
    let profile = resolve_tau_daemon_profile(config.profile);
    let state_path = daemon_state_path(&config.state_dir);
    let pid_path = daemon_pid_path(&config.state_dir);
    let mut state = load_tau_daemon_state(&state_path, profile)?;

    if pid_path.exists() {
        std::fs::remove_file(&pid_path)
            .with_context(|| format!("failed to remove daemon pid file {}", pid_path.display()))?;
    }

    state.running = false;
    state.pid = None;
    state.last_stop_unix_ms = Some(current_unix_timestamp_ms());
    state.stop_attempts = state.stop_attempts.saturating_add(1);
    state.last_stop_reason = reason
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| Some("daemon_stop".to_string()));
    save_tau_daemon_state(&state_path, &state)?;

    inspect_tau_daemon(config)
}

pub fn inspect_tau_daemon(config: &TauDaemonConfig) -> Result<TauDaemonStatusReport> {
    let profile = resolve_tau_daemon_profile(config.profile);
    let state_path = daemon_state_path(&config.state_dir);
    let pid_path = daemon_pid_path(&config.state_dir);
    let state = load_tau_daemon_state(&state_path, profile)?;

    let executable_path = resolve_executable_path();
    let service_file_exists = Path::new(&state.service_file_path).exists();
    let pid_file_exists = pid_path.exists();
    let state_dir_exists = config.state_dir.exists();
    let state_dir_writable = probe_state_dir_writable(&config.state_dir);
    let state_profile = CliDaemonProfile::from_str_label(state.profile.as_str()).unwrap_or(profile);

    let mut diagnostics = Vec::new();
    if !state_profile.supported_on_host() {
        diagnostics.push("profile_not_supported_on_host".to_string());
    }
    if !service_file_exists && state.installed {
        diagnostics.push("service_file_missing".to_string());
    }
    if state.running && !pid_file_exists {
        diagnostics.push("pid_file_missing_for_running_state".to_string());
    }
    if !state_dir_writable {
        diagnostics.push("state_dir_not_writable".to_string());
    }
    if !executable_path.exists() {
        diagnostics.push("executable_missing".to_string());
    }

    Ok(TauDaemonStatusReport {
        schema_version: TAU_DAEMON_STATE_SCHEMA_VERSION,
        state_path: state_path.display().to_string(),
        installed: state.installed,
        running: state.running && pid_file_exists,
        profile: state.profile,
        service_label: state.service_label,
        service_file_path: state.service_file_path,
        service_file_exists,
        pid_file_path: pid_path.display().to_string(),
        pid_file_exists,
        pid: if pid_file_exists { state.pid } else { None },
        host_os: std::env::consts::OS.to_string(),
        profile_supported_on_host: state_profile.supported_on_host(),
        executable_path: executable_path.display().to_string(),
        executable_exists: executable_path.exists(),
        state_dir_exists,
        state_dir_writable,
        last_install_unix_ms: state.last_install_unix_ms,
        last_start_unix_ms: state.last_start_unix_ms,
        last_stop_unix_ms: state.last_stop_unix_ms,
        last_stop_reason: state.last_stop_reason,
        start_attempts: state.start_attempts,
        stop_attempts: state.stop_attempts,
        diagnostics,
    })
}

pub fn render_tau_daemon_status_report(report: &TauDaemonStatusReport) -> String {
    format!(
        "tau daemon status: state_path={} installed={} running={} profile={} profile_supported_on_host={} service_label={} service_file={} service_file_exists={} pid_file={} pid_file_exists={} pid={} executable={} executable_exists={} state_dir_exists={} state_dir_writable={} start_attempts={} stop_attempts={} last_install_unix_ms={} last_start_unix_ms={} last_stop_unix_ms={} last_stop_reason={} diagnostics={}",
        report.state_path,
        report.installed,
        report.running,
        report.profile,
        report.profile_supported_on_host,
        report.service_label,
        report.service_file_path,
        report.service_file_exists,
        report.pid_file_path,
        report.pid_file_exists,
        report
            .pid
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        report.executable_path,
        report.executable_exists,
        report.state_dir_exists,
        report.state_dir_writable,
        report.start_attempts,
        report.stop_attempts,
        report
            .last_install_unix_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        report
            .last_start_unix_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        report
            .last_stop_unix_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        report
            .last_stop_reason
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("none"),
        if report.diagnostics.is_empty() {
            "none".to_string()
        } else {
            report.diagnostics.join(",")
        },
    )
}

pub fn render_launchd_plist(label: &str, executable: &Path, state_dir: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{label}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{executable}</string>
    <string>--model</string>
    <string>openai/gpt-4o-mini</string>
    <string>--gateway-openresponses-server</string>
    <string>--gateway-openresponses-auth-mode</string>
    <string>localhost-dev</string>
    <string>--gateway-openresponses-bind</string>
    <string>127.0.0.1:8787</string>
    <string>--gateway-state-dir</string>
    <string>{state_dir}/gateway</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardOutPath</key>
  <string>{state_dir}/logs/stdout.log</string>
  <key>StandardErrorPath</key>
  <string>{state_dir}/logs/stderr.log</string>
</dict>
</plist>
"#,
        label = label,
        executable = executable.display(),
        state_dir = state_dir.display(),
    )
}

pub fn render_systemd_user_unit(label: &str, executable: &Path, state_dir: &Path) -> String {
    format!(
        r#"[Unit]
Description=Tau Coding Agent daemon ({label})
After=network.target

[Service]
Type=simple
ExecStart={executable} --model openai/gpt-4o-mini --gateway-openresponses-server --gateway-openresponses-auth-mode localhost-dev --gateway-openresponses-bind 127.0.0.1:8787 --gateway-state-dir {state_dir}/gateway
Restart=on-failure
RestartSec=2
WorkingDirectory={state_dir}
StandardOutput=append:{state_dir}/logs/stdout.log
StandardError=append:{state_dir}/logs/stderr.log

[Install]
WantedBy=default.target
"#,
        label = label,
        executable = executable.display(),
        state_dir = state_dir.display(),
    )
}

fn daemon_state_path(state_dir: &Path) -> PathBuf {
    state_dir.join(DAEMON_STATE_FILE_NAME)
}

fn daemon_pid_path(state_dir: &Path) -> PathBuf {
    state_dir.join(DAEMON_PID_FILE_NAME)
}

fn daemon_service_file_path(state_dir: &Path, profile: CliDaemonProfile) -> PathBuf {
    match profile {
        CliDaemonProfile::Launchd => state_dir
            .join("launchd")
            .join(format!("{TAU_DAEMON_SERVICE_LABEL}.plist")),
        CliDaemonProfile::SystemdUser => state_dir.join("systemd").join("tau-coding-agent.service"),
        CliDaemonProfile::Auto => daemon_service_file_path(
            state_dir,
            resolve_tau_daemon_profile(CliDaemonProfile::Auto),
        ),
    }
}

fn load_tau_daemon_state(
    state_path: &Path,
    profile: CliDaemonProfile,
) -> Result<TauDaemonLifecycleState> {
    let default_service_path = daemon_service_file_path(
        state_path.parent().unwrap_or_else(|| Path::new(".")),
        profile,
    );
    if !state_path.exists() {
        return Ok(TauDaemonLifecycleState::default_with_service_path(
            profile,
            &default_service_path,
        ));
    }

    let raw = std::fs::read_to_string(state_path)
        .with_context(|| format!("failed to read daemon state {}", state_path.display()))?;
    let mut state = serde_json::from_str::<TauDaemonLifecycleState>(&raw)
        .with_context(|| format!("failed to parse daemon state {}", state_path.display()))?;
    if state.schema_version != TAU_DAEMON_STATE_SCHEMA_VERSION {
        bail!(
            "unsupported daemon state schema version {} (expected {})",
            state.schema_version,
            TAU_DAEMON_STATE_SCHEMA_VERSION
        );
    }
    if state.service_file_path.trim().is_empty() {
        state.service_file_path = default_service_path.display().to_string();
    }
    if state.profile.trim().is_empty() {
        state.profile = profile.as_str().to_string();
    }
    if state.service_label.trim().is_empty() {
        state.service_label = TAU_DAEMON_SERVICE_LABEL.to_string();
    }
    Ok(state)
}

fn save_tau_daemon_state(state_path: &Path, state: &TauDaemonLifecycleState) -> Result<()> {
    write_text_atomic(
        state_path,
        serde_json::to_string_pretty(state)
            .context("failed to serialize daemon state")?
            .as_str(),
    )
    .with_context(|| format!("failed to persist daemon state {}", state_path.display()))
}

fn resolve_executable_path() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("tau-coding-agent"))
}

fn probe_state_dir_writable(state_dir: &Path) -> bool {
    if std::fs::create_dir_all(state_dir).is_err() {
        return false;
    }
    let probe = state_dir.join(".daemon-write-probe");
    if std::fs::write(&probe, "probe").is_err() {
        return false;
    }
    let _ = std::fs::remove_file(&probe);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn unit_render_launchd_plist_includes_expected_command_and_paths() {
        let executable = Path::new("/usr/local/bin/tau-coding-agent");
        let state_dir = Path::new("/tmp/tau-daemon");
        let rendered = render_launchd_plist("io.tau.coding-agent", executable, state_dir);
        assert!(rendered.contains("io.tau.coding-agent"));
        assert!(rendered.contains("/usr/local/bin/tau-coding-agent"));
        assert!(rendered.contains("--gateway-openresponses-server"));
        assert!(rendered.contains("/tmp/tau-daemon/gateway"));
    }

    #[test]
    fn unit_render_systemd_user_unit_includes_expected_command_and_paths() {
        let executable = Path::new("/usr/bin/tau-coding-agent");
        let state_dir = Path::new("/var/tmp/tau-daemon");
        let rendered = render_systemd_user_unit("io.tau.coding-agent", executable, state_dir);
        assert!(rendered.contains("Description=Tau Coding Agent daemon (io.tau.coding-agent)"));
        assert!(rendered.contains("ExecStart=/usr/bin/tau-coding-agent"));
        assert!(rendered.contains("--gateway-openresponses-server"));
        assert!(rendered.contains("/var/tmp/tau-daemon/gateway"));
    }

    #[test]
    fn functional_tau_daemon_lifecycle_install_start_stop_uninstall_roundtrip() {
        let temp = tempdir().expect("tempdir");
        let config = TauDaemonConfig {
            state_dir: temp.path().join(".tau/daemon"),
            profile: CliDaemonProfile::SystemdUser,
        };

        let installed = install_tau_daemon(&config).expect("install daemon");
        assert!(installed.installed);
        assert!(!installed.running);
        assert!(installed.service_file_exists);
        assert!(Path::new(installed.service_file_path.as_str()).exists());

        let started = start_tau_daemon(&config).expect("start daemon");
        assert!(started.installed);
        assert!(started.running);
        assert!(started.pid.is_some());
        assert!(started.pid_file_exists);

        let stopped = stop_tau_daemon(&config, Some("maintenance_window")).expect("stop daemon");
        assert!(!stopped.running);
        assert_eq!(
            stopped.last_stop_reason.as_deref(),
            Some("maintenance_window")
        );
        assert!(!stopped.pid_file_exists);

        let uninstalled = uninstall_tau_daemon(&config).expect("uninstall daemon");
        assert!(!uninstalled.installed);
        assert!(!uninstalled.running);
        assert!(!uninstalled.service_file_exists);
    }

    #[test]
    fn integration_tau_daemon_status_report_is_deterministic_across_reloads() {
        let temp = tempdir().expect("tempdir");
        let config = TauDaemonConfig {
            state_dir: temp.path().join(".tau/daemon"),
            profile: CliDaemonProfile::SystemdUser,
        };

        install_tau_daemon(&config).expect("install daemon");
        start_tau_daemon(&config).expect("start daemon");
        let first = inspect_tau_daemon(&config).expect("first status");
        let second = inspect_tau_daemon(&config).expect("second status");
        assert_eq!(first.installed, second.installed);
        assert_eq!(first.running, second.running);
        assert_eq!(first.profile, second.profile);
        assert_eq!(first.service_file_path, second.service_file_path);
        assert_eq!(first.pid_file_exists, second.pid_file_exists);
        assert_eq!(
            first.profile_supported_on_host,
            second.profile_supported_on_host
        );
    }

    #[test]
    fn regression_tau_daemon_start_requires_install() {
        let temp = tempdir().expect("tempdir");
        let config = TauDaemonConfig {
            state_dir: temp.path().join(".tau/daemon"),
            profile: CliDaemonProfile::SystemdUser,
        };

        let error = start_tau_daemon(&config).expect_err("start without install should fail");
        assert!(error.to_string().contains("run --daemon-install first"));
    }

    #[test]
    fn regression_tau_daemon_status_reports_stale_running_state_when_pid_file_missing() {
        let temp = tempdir().expect("tempdir");
        let config = TauDaemonConfig {
            state_dir: temp.path().join(".tau/daemon"),
            profile: CliDaemonProfile::SystemdUser,
        };

        install_tau_daemon(&config).expect("install daemon");
        start_tau_daemon(&config).expect("start daemon");
        let pid_path = daemon_pid_path(&config.state_dir);
        std::fs::remove_file(&pid_path).expect("remove pid file");

        let report = inspect_tau_daemon(&config).expect("inspect status");
        assert!(!report.running);
        assert!(report
            .diagnostics
            .contains(&"pid_file_missing_for_running_state".to_string()));
    }
}
