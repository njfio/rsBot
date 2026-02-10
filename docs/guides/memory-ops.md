# Memory Operations Runbook

Run all commands from repository root.

## Scope

This runbook covers the fixture-driven semantic memory runtime (`--memory-contract-runner`).

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

## Rollout plan with guardrails

1. Validate fixture contract and runtime locally:
   `cargo test -p tau-coding-agent memory_contract -- --test-threads=1`
2. Validate runtime coverage:
   `cargo test -p tau-coding-agent memory_runtime -- --test-threads=1`
3. Run deterministic demo:
   `./scripts/demo/memory.sh`
4. Confirm health snapshot is `healthy` before promotion:
   `--transport-health-inspect memory --transport-health-json`
5. Promote by increasing fixture complexity gradually while monitoring:
   `failure_streak`, `last_cycle_failed`, `queue_depth`.

## Rollback plan

1. Stop invoking `--memory-contract-runner`.
2. Preserve `.tau/memory/` for incident analysis.
3. Revert to last known-good revision:
   `git revert <commit>`
4. Re-run validation matrix before re-enable.

## Troubleshooting

- Symptom: health state `degraded` with `case_processing_failed`.
  Action: inspect `runtime-events.jsonl`, then verify fixture expectations and channel-store write permissions.
- Symptom: health state `degraded` with `retry_attempted`.
  Action: inspect `simulate_retryable_failure` fixture paths and adjust retry controls only when transient errors are expected.
- Symptom: health state `failing` (`failure_streak >= 3`).
  Action: treat as rollout gate failure; pause promotion and investigate repeated runtime failures.
- Symptom: non-zero `queue_depth`.
  Action: increase `--memory-queue-limit` or reduce per-cycle fixture volume.
