# Dashboard Operations Runbook

Run all commands from repository root.

## Scope

This runbook covers the fixture-driven dashboard runtime (`--dashboard-contract-runner`).

## Health and observability signals

Primary transport health signal:

```bash
cargo run -p tau-coding-agent -- \
  --dashboard-state-dir .tau/dashboard \
  --transport-health-inspect dashboard \
  --transport-health-json
```

Primary operator status/guardrail signal:

```bash
cargo run -p tau-coding-agent -- \
  --dashboard-state-dir .tau/dashboard \
  --dashboard-status-inspect \
  --dashboard-status-json
```

Primary state files:

- `.tau/dashboard/state.json`
- `.tau/dashboard/runtime-events.jsonl`
- `.tau/dashboard/channel-store/dashboard/<channel_id>/...`

`runtime-events.jsonl` reason codes:

- `healthy_cycle`
- `queue_backpressure_applied`
- `duplicate_cases_skipped`
- `malformed_inputs_observed`
- `retry_attempted`
- `retryable_failures_observed`
- `case_processing_failed`
- `widget_views_updated`
- `control_actions_applied`

Guardrail interpretation:

- `rollout_gate=pass`: health is `healthy`, promotion can continue.
- `rollout_gate=hold`: health is `degraded` or `failing`, pause promotion and investigate.

## Deterministic demo path

```bash
./scripts/demo/dashboard.sh
```

## Rollout plan with guardrails

1. Validate fixture contract and runtime locally:
   `cargo test -p tau-coding-agent dashboard_contract -- --test-threads=1`
2. Validate runtime behavior coverage:
   `cargo test -p tau-coding-agent dashboard_runtime -- --test-threads=1`
3. Run deterministic demo:
   `./scripts/demo/dashboard.sh`
4. Verify transport health and status gate:
   `--transport-health-inspect dashboard --transport-health-json`
   `--dashboard-status-inspect --dashboard-status-json`
5. Promote by increasing fixture complexity gradually while monitoring:
   `failure_streak`, `last_cycle_failed`, `queue_depth`, `rollout_gate`.

## Canary rollout profile

Apply the global rollout contract in [Release Channel Ops](release-channel-ops.md#cross-surface-rollout-contract).

| Phase | Canary % | Dashboard-specific gates |
| --- | --- | --- |
| canary-1 | 5% | `rollout_gate=pass`, `health_state=healthy`, `failure_streak=0`, `queue_depth<=1`, no new `case_processing_failed`. |
| canary-2 | 25% | canary-1 gates hold for 60 minutes; `widget_views_updated` and `control_actions_applied` continue to advance. |
| canary-3 | 50% | canary-2 gates hold for 120 minutes; retry-related reason codes remain flat. |
| general-availability | 100% | 24-hour monitor window passes and release sign-off checklist is complete. |

## Rollback plan

1. Stop invoking `--dashboard-contract-runner`.
2. Preserve `.tau/dashboard/` for incident analysis.
3. Revert to last known-good revision:
   `git revert <commit>`
4. Re-run validation matrix before re-enable.
5. If any rollback trigger from [Rollback Trigger Matrix](release-channel-ops.md#rollback-trigger-matrix) fires, stop promotion immediately and execute [Rollback Execution Steps](release-channel-ops.md#rollback-execution-steps).

## Troubleshooting

- Symptom: health state `degraded` with `case_processing_failed`.
  Action: inspect `runtime-events.jsonl`, then verify fixture expectations and output paths.
- Symptom: health state `degraded` with `retry_attempted`.
  Action: inspect fixture cases with simulated transient failures and retry controls.
- Symptom: health state `failing` (`failure_streak >= 3`).
  Action: treat as rollout gate failure; pause promotion and investigate repeated failures.
- Symptom: `rollout_gate=hold` with zero recent events.
  Action: run demo or targeted fixture to refresh runtime state and confirm status signal freshness.
- Symptom: non-zero `queue_depth`.
  Action: increase `--dashboard-queue-limit` or reduce fixture batch size to avoid backlog drops.
