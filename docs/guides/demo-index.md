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

Generate a standard M21 proof-pack manifest alongside the report:

```bash
./scripts/demo/index.sh \
  --json \
  --report-file .tau/reports/demo-index-summary.json
# auto-emits: .tau/reports/demo-index-summary.manifest.json
```

Override manifest destination explicitly:

```bash
./scripts/demo/index.sh \
  --json \
  --report-file .tau/reports/demo-index-summary.json \
  --manifest-file .tau/reports/m21-live-proof-pack.json
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

## CI-light smoke notes

The repository smoke manifest (`.github/demo-smoke-manifest.json`) includes lightweight
commands that cover the same workflow classes:
- onboarding bootstrap
- gateway auth posture inspect
- gateway remote plan export + fail-closed guardrail contract
- multi-channel live ingress (Telegram/Discord/WhatsApp)
- deployment WASM package and inspect

This keeps smoke runs deterministic while avoiding external-service dependencies.

## Proof-pack manifest schema

`scripts/demo/proof-pack-manifest.sh` defines the standard artifact manifest template used by
`scripts/demo/index.sh` and `scripts/demo/all.sh` whenever `--report-file` is set.

Required top-level fields:
- `schema_version` (integer)
- `generated_at` (UTC ISO-8601 timestamp)
- `pack_name` (logical proof-pack identifier)
- `milestone` (roadmap milestone label)
- `issues` (array of linked issue IDs)
- `producer.script` / `producer.mode` (emitting wrapper + list/run mode)
- `artifacts[]` with:
  - `name`
  - `path`
  - `required`
  - `status` (`present` or `missing`)
- `summary.status` (`pass`, `fail`, or `unknown`) with optional `total/passed/failed` counts

Reviewer checklist:
- confirm each required artifact entry is `present`
- confirm `summary.status == "pass"` for closure-ready proof packs
- verify `issues[]` links and `producer` metadata match the reviewed run

## Retained-capability proof summary collector

Use `scripts/dev/m21-retained-capability-proof-summary.sh` to execute the retained
proof run matrix and emit a machine-readable summary with exit/status/marker diagnostics.

Example with explicit binary:

```bash
./scripts/dev/m21-retained-capability-proof-summary.sh \
  --repo-root . \
  --binary ./target/debug/tau-coding-agent
```

Canonical retained-capability matrix and artifact checklist:
- `scripts/demo/m21-retained-capability-proof-matrix.json`
- capability IDs: `onboarding`, `gateway-auth`, `gateway-remote-access`,
  `multi-channel-live`, `deployment-wasm`
- checklist contract fields for each required artifact:
  `name`, `path`, `required`, `status`

Validate matrix/checklist contracts before live-proof runs:

```bash
./scripts/demo/validate-m21-retained-capability-proof-matrix.sh
```

Default report path conventions:
- JSON summary: `tasks/reports/m21-retained-capability-proof-summary.json`
- Markdown summary: `tasks/reports/m21-retained-capability-proof-summary.md`
- Per-run logs: `tasks/reports/m21-retained-capability-proof-logs/`
- Generated run artifacts: `tasks/reports/m21-retained-capability-artifacts/`

Each run entry in the JSON summary includes:
- command line
- expected vs actual exit code
- pass/fail status
- marker match results (`stdout`, `stderr`, `file`)
- stdout/stderr log paths for direct triage
