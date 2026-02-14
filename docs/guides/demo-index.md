# Demo Index Guide

Run all commands from repository root.

This guide provides a compact, reproducible demo suite for fresh-clone validation of
core Tau operator workflows.

## Quick start

```bash
./scripts/demo/index.sh
```

List available scenarios without execution:

```bash
./scripts/demo/index.sh --list
./scripts/demo/index.sh --list --json
```

Run only selected scenarios:

```bash
./scripts/demo/index.sh --only onboarding,gateway-auth,gateway-remote-access --fail-fast
```

Write deterministic JSON summary:

```bash
./scripts/demo/index.sh --json --report-file .tau/reports/demo-index-summary.json
```

## Scenario matrix

### onboarding
- Wrapper: `./scripts/demo/local.sh`
- Purpose: bootstrap first-run local Tau operator state and baseline commands.
- Expected markers:
  - `[demo:local] PASS onboard-non-interactive`
  - `[demo:local] summary: total=`
- Troubleshooting checkpoint:
  - confirm `.tau` is writable, then rerun `./scripts/demo/local.sh --fail-fast`.

### gateway-auth
- Wrapper: `./scripts/demo/gateway-auth.sh`
- Purpose: validate gateway auth posture for token and password-session remote profiles.
- Expected markers:
  - `[demo:gateway-auth] PASS gateway-remote-profile-token-mode`
  - `[demo:gateway-auth] PASS gateway-remote-profile-password-session-mode`
- Troubleshooting checkpoint:
  - verify `--gateway-openresponses-auth-mode` and required token/password flags.

### gateway-remote-access
- Wrapper: `./scripts/demo/gateway-remote-access.sh`
- Purpose: validate wave-9 profile inspect + remote-plan fail-closed guardrail behavior.
- Expected markers:
  - `[demo:gateway-remote-access] PASS gateway-remote-plan-export-tailscale-serve`
  - `[demo:gateway-remote-access] PASS gateway-remote-plan-fails-closed-for-missing-password`
- Troubleshooting checkpoint:
  - inspect `.tau/demo-gateway-remote-access/trace.log`, then verify auth/profile flags.

### multi-channel-live
- Wrapper: `./scripts/demo/multi-channel.sh`
- Purpose: ingest live fixtures and verify transport health for Telegram, Discord, and WhatsApp.
- Expected markers:
  - `[demo:multi-channel] PASS multi-channel-live-ingest-telegram`
  - `[demo:multi-channel] PASS multi-channel-live-ingest-discord`
  - `[demo:multi-channel] PASS multi-channel-live-ingest-whatsapp`
- Troubleshooting checkpoint:
  - verify fixture files in `crates/tau-multi-channel/testdata/multi-channel-live-ingress/raw`.

### deployment-wasm
- Wrapper: `./scripts/demo/deployment.sh`
- Purpose: package WASM deployment artifact and verify inspect/status outputs.
- Expected markers:
  - `[demo:deployment] PASS deployment-wasm-package`
  - `[demo:deployment] PASS channel-store-inspect-deployment-edge-wasm`
- Troubleshooting checkpoint:
  - verify deployment fixture paths and WASM module file in `testdata/deployment-wasm`.

## Standalone Live Harness

`browser-automation-live` is a standalone wrapper (not part of `index.sh` scenario allowlist)
used for live browser timeline/artifact proof runs:

```bash
./scripts/demo/browser-automation-live.sh
```

Primary outputs:

- `.tau/demo-browser-automation-live/browser-live-summary.json`
- `.tau/demo-browser-automation-live/browser-live-report.json`
- `.tau/demo-browser-automation-live/browser-live-transcript.log`

## Unified Live-Run Harness

Cross-surface validation wrapper (voice/browser/dashboard/custom-command/memory):

```bash
./scripts/demo/live-run-unified.sh
```

Deterministic inventory mode:

```bash
./scripts/demo/live-run-unified.sh --list
./scripts/demo/live-run-unified.sh --list --json
```

Primary outputs:

- `.tau/live-run-unified/manifest.json`
- `.tau/live-run-unified/report.json`
- `.tau/live-run-unified/surfaces/<surface>/stdout.log`
- `.tau/live-run-unified/surfaces/<surface>/stderr.log`

## CI-light smoke notes

The repository smoke manifest (`.github/demo-smoke-manifest.json`) includes lightweight
commands that cover the same workflow classes:
- onboarding bootstrap
- gateway auth posture inspect
- gateway remote plan export + fail-closed guardrail contract
- multi-channel live ingress (Telegram/Discord/WhatsApp)
- deployment WASM package and inspect

This keeps smoke runs deterministic while avoiding external-service dependencies.
