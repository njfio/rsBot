# Operator Control Summary

Run from repository root.

## Purpose

`--operator-control-summary` gives one day-2 control-plane view that combines:
- transport and runtime health for dashboard, multi-channel, multi-agent, gateway, deployment, custom-command, and voice
- gateway remote-access policy posture
- daemon lifecycle state
- release-channel state

## Commands

Text summary:

```bash
cargo run -p tau-coding-agent -- \
  --operator-control-summary
```

JSON summary:

```bash
cargo run -p tau-coding-agent -- \
  --operator-control-summary \
  --operator-control-summary-json
```

Capture baseline snapshot:

```bash
cargo run -p tau-coding-agent -- \
  --operator-control-summary \
  --operator-control-summary-snapshot-out .tau/reports/operator-control-baseline.json
```

Compare current state against baseline snapshot:

```bash
cargo run -p tau-coding-agent -- \
  --operator-control-summary \
  --operator-control-summary-compare .tau/reports/operator-control-baseline.json
```

Compare in JSON mode:

```bash
cargo run -p tau-coding-agent -- \
  --operator-control-summary \
  --operator-control-summary-json \
  --operator-control-summary-compare .tau/reports/operator-control-baseline.json
```

## Output shape

Top-level fields:
- `health_state`: `healthy|degraded|failing`
- `rollout_gate`: `pass|hold`
- `reason_codes`: aggregate hold reasons across components
- `recommendations`: actionable guidance for current hold/degraded/failing conditions
- `policy_posture`: pairing strictness, gateway auth mode, remote profile posture
- `daemon`: daemon health and lifecycle posture
- `release_channel`: release-channel configuration posture
- `components[]`: per-component health rows with reason/recommendation and queue/failure counters

When `--operator-control-summary-compare` is used:
- `drift_state`: `stable|changed|improved|regressed`
- `risk_level`: `low|moderate|high`
- `reason_codes_added|reason_codes_removed`: aggregate reason-code deltas
- `recommendations_added|recommendations_removed`: recommendation deltas
- `changed_components[]`: per-component drift rows (`severity`, before/after state, queue/failure counters)
- `unchanged_component_count`: stable component count

## Troubleshooting map

Common hold reason codes and actions:
- `*:state_unavailable`
  - action: initialize or repair component state (`state.json`) and rerun summary
- `gateway:service_stopped`
  - action: start gateway service mode (`--gateway-service-start`) before resuming traffic
- `daemon:daemon_not_installed`
  - action: install daemon (`--daemon-install`) if background lifecycle management is required
- `daemon:daemon_not_running`
  - action: start daemon (`--daemon-start`) and verify with `--daemon-status --daemon-status-json`
- `release-channel:release_channel_missing`
  - action: set release channel with `/release-channel set <stable|beta|dev>`
- `gateway-remote-profile:*`
  - action: run `--gateway-remote-profile-inspect` and apply the recommended bind/auth/profile fixes

When `health_state=failing`:
1. Resolve `reason_codes` in listed order.
2. Re-run `--operator-control-summary --operator-control-summary-json`.
3. Confirm `rollout_gate=pass` before promoting runtime changes.
