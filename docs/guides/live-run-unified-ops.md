# Unified Live-Run Harness Guide

Run all commands from repository root.

This harness executes live validation flows across:

- voice
- browser automation
- dashboard
- custom commands
- memory

## Run Unified Harness

```bash
./scripts/demo/live-run-unified.sh
```

Fast inventory check without execution:

```bash
./scripts/demo/live-run-unified.sh --list
./scripts/demo/live-run-unified.sh --list --json
```

Run with bounded wrapper timeout and continue after one surface fails:

```bash
./scripts/demo/live-run-unified.sh --timeout-seconds 120 --keep-going
```

## Manifest + Report Contract

Outputs are written under:

- `.tau/live-run-unified/manifest.json`
- `.tau/live-run-unified/report.json`
- `.tau/live-run-unified/surfaces/<surface-id>/stdout.log`
- `.tau/live-run-unified/surfaces/<surface-id>/stderr.log`
- `.tau/live-run-unified/surfaces/<surface-id>/artifacts/...`

`manifest.json` includes:

- `schema_version`
- `overall` (`status`, `total_surfaces`, `passed_surfaces`, `failed_surfaces`)
- per-surface run records (`status`, `exit_code`, `duration_ms`, logs, copied artifact inventory, diagnostics)

`report.json` includes:

- aggregated overall summary
- per-surface status map
- failed surface IDs

## Surface Manifest

The default surface orchestration file is:

- `.github/live-run-unified-manifest.json`

Each entry defines:

- `id`
- `script`
- `artifact_roots`

## Triage Workflow

1. Inspect aggregate result:
   - `cat .tau/live-run-unified/report.json`
2. Inspect detailed contract:
   - `cat .tau/live-run-unified/manifest.json`
3. For a failed surface, inspect:
   - `cat .tau/live-run-unified/surfaces/<surface-id>/stdout.log`
   - `cat .tau/live-run-unified/surfaces/<surface-id>/stderr.log`
4. Validate copied artifact inventory and checksums in `manifest.json`.

## CI Artifact Set

Attach these files for rollout-signoff evidence:

- `.tau/live-run-unified/manifest.json`
- `.tau/live-run-unified/report.json`
- `.tau/live-run-unified/surfaces/**/stdout.log`
- `.tau/live-run-unified/surfaces/**/stderr.log`

## CI Gate Policy

CI workflow `CI` includes:

- `live-smoke-matrix` (surface matrix: voice/browser/dashboard/custom-command/memory)
- `live-smoke-gate` (fails merge gate when required surface smoke fails)

Browser matrix lane includes fallback behavior:

- primary: `scripts/demo/browser-automation-live.sh`
- fallback: `scripts/demo/browser-automation.sh`

Merge gate blocks when:

- any required surface fails both primary and fallback mode, or
- matrix job exits non-success.
