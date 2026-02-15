# Tau Daemon Ops Runbook

This runbook covers Tau daemon lifecycle operations and profile troubleshooting.

## Scope

- Lifecycle commands: install, uninstall, start, stop, status.
- Profile targets: launchd (macOS) and systemd user-mode (Linux).
- State root: `--daemon-state-dir` (default `.tau/daemon`).
- Onboarding bootstrap flags: `--onboard-install-daemon`, `--onboard-start-daemon`.

## Onboarding bootstrap

Non-interactive onboarding with daemon install/start:

```bash
cargo run -p tau-coding-agent -- --onboard --onboard-non-interactive --onboard-install-daemon --onboard-start-daemon
```

Interactive onboarding with daemon install only:

```bash
cargo run -p tau-coding-agent -- --onboard --onboard-profile default --onboard-install-daemon
```

## Lifecycle commands

Install profile files:

```bash
cargo run -p tau-coding-agent -- --daemon-install --daemon-profile auto --daemon-state-dir .tau/daemon
```

Start lifecycle state:

```bash
cargo run -p tau-coding-agent -- --daemon-start --daemon-state-dir .tau/daemon
```

Stop lifecycle state:

```bash
cargo run -p tau-coding-agent -- --daemon-stop --daemon-stop-reason maintenance_window --daemon-state-dir .tau/daemon
```

Inspect diagnostics:

```bash
cargo run -p tau-coding-agent -- --daemon-status --daemon-status-json --daemon-state-dir .tau/daemon
```

Uninstall profile files:

```bash
cargo run -p tau-coding-agent -- --daemon-uninstall --daemon-state-dir .tau/daemon
```

## Generated files

- State: `.tau/daemon/state.json`
- PID marker: `.tau/daemon/daemon.pid`
- launchd profile: `.tau/daemon/launchd/io.tau.coding-agent.plist`
- systemd user profile: `.tau/daemon/systemd/tau-coding-agent.service`

## Runtime heartbeat controls

Local/runtime daemon sessions can run periodic maintenance probes (queues/events/jobs) with:

- `--runtime-heartbeat-enabled` (default `true`)
- `--runtime-heartbeat-interval-ms` (default `5000`)
- `--runtime-heartbeat-state-path` (default `.tau/runtime-heartbeat/state.json`)
- `--runtime-self-repair-enabled` (default `true`)
- `--runtime-self-repair-timeout-ms` (default `300000`)
- `--runtime-self-repair-max-retries` (default `2`)
- `--runtime-self-repair-tool-builds-dir` (default `.tau/tool-builds`)
- `--runtime-self-repair-orphan-max-age-seconds` (default `3600`)

Example:

```bash
cargo run -p tau-coding-agent -- \
  --daemon-start \
  --runtime-heartbeat-enabled=true \
  --runtime-heartbeat-interval-ms 3000 \
  --runtime-heartbeat-state-path .tau/runtime-heartbeat/state.json \
  --runtime-self-repair-enabled=true \
  --runtime-self-repair-timeout-ms 60000 \
  --runtime-self-repair-max-retries 2 \
  --runtime-self-repair-tool-builds-dir .tau/tool-builds \
  --runtime-self-repair-orphan-max-age-seconds 3600
```

## Activate launchd profile (macOS)

```bash
mkdir -p ~/Library/LaunchAgents
cp .tau/daemon/launchd/io.tau.coding-agent.plist ~/Library/LaunchAgents/
launchctl bootstrap "gui/$(id -u)" ~/Library/LaunchAgents/io.tau.coding-agent.plist
launchctl kickstart -k "gui/$(id -u)/io.tau.coding-agent"
launchctl print "gui/$(id -u)/io.tau.coding-agent"
```

Disable/remove launchd profile:

```bash
launchctl bootout "gui/$(id -u)/io.tau.coding-agent"
rm -f ~/Library/LaunchAgents/io.tau.coding-agent.plist
```

## Activate systemd user profile (Linux)

```bash
mkdir -p ~/.config/systemd/user
cp .tau/daemon/systemd/tau-coding-agent.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now tau-coding-agent.service
systemctl --user status tau-coding-agent.service --no-pager
```

Disable/remove systemd user profile:

```bash
systemctl --user disable --now tau-coding-agent.service
rm -f ~/.config/systemd/user/tau-coding-agent.service
systemctl --user daemon-reload
```

## Troubleshooting

If `profile_not_supported_on_host` appears:
- Use `--daemon-profile auto` or select the host-supported profile.

If `service_file_missing` appears:
- Re-run install: `--daemon-install`.

If `pid_file_missing_for_running_state` appears:
- Reconcile lifecycle state with `--daemon-stop` then `--daemon-start`.

If `state_dir_not_writable` appears:
- Check permissions/ownership for `--daemon-state-dir` and parent directories.

If `executable_missing` appears:
- Reinstall/build Tau binary and re-run `--daemon-install`.
