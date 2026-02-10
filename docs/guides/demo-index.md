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
./scripts/demo/index.sh --only onboarding,gateway-auth --fail-fast
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

## CI-light smoke notes

The repository smoke manifest (`.github/demo-smoke-manifest.json`) includes lightweight
commands that cover the same workflow classes:
- onboarding bootstrap
- gateway auth posture inspect
- multi-channel live ingress (Telegram/Discord/WhatsApp)
- deployment WASM package and inspect

This keeps smoke runs deterministic while avoiding external-service dependencies.
