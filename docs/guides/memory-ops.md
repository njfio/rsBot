# Memory Operations Runbook

Run all commands from repository root.

## Scope

This runbook covers:

- fixture-driven semantic memory runtime (`--memory-contract-runner`)
- live backend recall quality proof (`scripts/demo/memory-live.sh`)

## Health and observability signals

Primary status signal:

```bash
cargo run -p tau-coding-agent -- \
  --memory-state-dir .tau/memory \
  --transport-health-inspect memory \
  --transport-health-json
```

Primary state files:

- `.tau/memory/state.json`
- `.tau/memory/runtime-events.jsonl`
- `.tau/memory/channel-store/memory/<channel_id>/...`

`runtime-events.jsonl` reason codes:

- `healthy_cycle`
- `queue_backpressure_applied`
- `duplicate_cases_skipped`
- `malformed_inputs_observed`
- `retry_attempted`
- `retryable_failures_observed`
- `case_processing_failed`

## Deterministic demo path

```bash
./scripts/demo/memory.sh
```

## Live backend quality proof

```bash
./scripts/demo/memory-live.sh
```

Primary proof artifacts:

- `.tau/demo-memory-live/memory-live-summary.json`
- `.tau/demo-memory-live/memory-live-quality-report.json`
- `.tau/demo-memory-live/memory-live-artifact-manifest.json`
- `.tau/demo-memory-live/memory-live-report.json`
- `.tau/demo-memory-live/memory-live-request-captures.json`
- `.tau/demo-memory-live/state/live-backend/<workspace>.jsonl`

## Rollout plan with guardrails

1. Validate fixture contract and runtime locally:
   `cargo test -p tau-coding-agent memory_contract -- --test-threads=1`
2. Validate runtime coverage:
   `cargo test -p tau-coding-agent memory_runtime -- --test-threads=1`
3. Validate live memory backend retrieval quality and artifact manifest generation:
   `./scripts/demo/memory-live.sh --skip-build --timeout-seconds 120`
4. Run deterministic fixture demo:
   `./scripts/demo/memory.sh`
5. Confirm health snapshot is `healthy` before promotion:
   `--transport-health-inspect memory --transport-health-json`
6. Promote by increasing fixture complexity gradually while monitoring:
   `failure_streak`, `last_cycle_failed`, `queue_depth`.

## Canary rollout profile

Apply the global rollout contract in [Release Channel Ops](release-channel-ops.md#cross-surface-rollout-contract).

| Phase | Canary % | Memory-specific gates |
| --- | --- | --- |
| canary-1 | 5% | `health_state=healthy`, `failure_streak=0`, `last_cycle_failed=false`, `queue_depth<=1`, no new `case_processing_failed`. |
| canary-2 | 25% | canary-1 gates hold for 60 minutes; duplicate and malformed counts remain flat. |
| canary-3 | 50% | canary-2 gates hold for 120 minutes; retry-related reason codes remain flat. |
| general-availability | 100% | 24-hour monitor window passes and release sign-off checklist is complete. |

## Rollback plan

1. Stop invoking `--memory-contract-runner`.
2. Preserve `.tau/memory/` for incident analysis.
3. Revert to last known-good revision:
   `git revert <commit>`
4. Re-run validation matrix before re-enable.
5. If any rollback trigger from [Rollback Trigger Matrix](release-channel-ops.md#rollback-trigger-matrix) fires, stop promotion immediately and execute [Rollback Execution Steps](release-channel-ops.md#rollback-execution-steps).

## Troubleshooting

- Symptom: health state `degraded` with `case_processing_failed`.
  Action: inspect `runtime-events.jsonl`, then verify fixture expectations and channel-store write permissions.
- Symptom: health state `degraded` with `retry_attempted`.
  Action: inspect `simulate_retryable_failure` fixture paths and adjust retry controls only when transient errors are expected.
- Symptom: health state `failing` (`failure_streak >= 3`).
  Action: treat as rollout gate failure; pause promotion and investigate repeated runtime failures.
- Symptom: non-zero `queue_depth`.
  Action: increase `--memory-queue-limit` or reduce per-cycle fixture volume.
- Symptom: `memory-live` report fails quality gate.
  Action: inspect `.tau/demo-memory-live/memory-live-quality-report.json` and request captures to verify recall ordering drift.
