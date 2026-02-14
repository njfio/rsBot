# Browser Automation Live Ops Guide

Run all commands from repository root.

This guide covers the deterministic live browser harness used for local and CI proof runs.

## Run Live Harness

```bash
./scripts/demo/browser-automation-live.sh
```

Optional bounded execution:

```bash
./scripts/demo/browser-automation-live.sh --timeout-seconds 120
```

Use an external Playwright CLI wrapper instead of the deterministic mock fallback:

```bash
./scripts/demo/browser-automation-live.sh --playwright-cli /path/to/playwright-wrapper
```

## Output Artifacts

The harness writes:

- `.tau/demo-browser-automation-live/browser-live-summary.json`
- `.tau/demo-browser-automation-live/browser-live-report.json`
- `.tau/demo-browser-automation-live/browser-live-transcript.log`
- `.tau/demo-browser-automation-live/state/channel-store/channels/browser-automation/live/log.jsonl`
- `.tau/demo-browser-automation-live/state/channel-store/channels/browser-automation/live/artifacts/index.jsonl`

`browser-live-summary.json` contains health counters and per-case timeline entries:

- `health_state`
- `reason_codes`
- `artifact_records`
- `timeline` (case order, replay step, status, error code, artifact types)

## Triage Workflow

1. Check summary health:
   - `cat .tau/demo-browser-automation-live/browser-live-summary.json`
2. Check transcript for failing step:
   - `cat .tau/demo-browser-automation-live/browser-live-transcript.log`
3. Inspect channel-store event log:
   - `cat .tau/demo-browser-automation-live/state/channel-store/channels/browser-automation/live/log.jsonl`
4. Inspect artifact index:
   - `cat .tau/demo-browser-automation-live/state/channel-store/channels/browser-automation/live/artifacts/index.jsonl`

Common failure signals:

- Harness binary missing: run without `--skip-build` so `cargo build` compiles it.
- Timeout: increase `--timeout-seconds`.
- Playwright wrapper issue: validate executable path passed to `--playwright-cli`.
- Policy denials: inspect `reason_codes` and timeline `error_code` fields.

## CI Attachment Pattern

Publish these files as CI artifacts for merge evidence:

- `.tau/demo-browser-automation-live/browser-live-summary.json`
- `.tau/demo-browser-automation-live/browser-live-report.json`
- `.tau/demo-browser-automation-live/browser-live-transcript.log`
