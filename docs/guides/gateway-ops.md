# Gateway Operations Runbook

Run all commands from repository root.

## Scope

This runbook covers the fixture-driven gateway runtime (`--gateway-contract-runner`).

## Health and observability signals

Primary transport health signal:

```bash
cargo run -p tau-coding-agent -- \
  --gateway-state-dir .tau/gateway \
  --transport-health-inspect gateway \
  --transport-health-json
```

Primary operator status/guardrail signal:

```bash
cargo run -p tau-coding-agent -- \
  --gateway-state-dir .tau/gateway \
  --gateway-status-inspect \
  --gateway-status-json
```

Primary state files:

- `.tau/gateway/state.json`
- `.tau/gateway/runtime-events.jsonl`
- `.tau/gateway/channel-store/gateway/<actor_id>/...`

`runtime-events.jsonl` reason codes:

- `healthy_cycle`
- `queue_backpressure_applied`
- `duplicate_cases_skipped`
- `malformed_inputs_observed`
- `retry_attempted`
- `retryable_failures_observed`
- `case_processing_failed`

Guardrail interpretation:

- `rollout_gate=pass`: guardrail thresholds are satisfied, promotion can continue.
- `rollout_gate=hold`: a guardrail threshold is breached; inspect `rollout_reason_code`.

Configurable guardrail thresholds (runner flags):

- `--gateway-guardrail-failure-streak-threshold` (default `2`)
- `--gateway-guardrail-retryable-failures-threshold` (default `2`)

## Deterministic demo path

```bash
./scripts/demo/gateway.sh
```

## Rollout plan with guardrails

1. Validate fixture contract and runtime locally:
   `cargo test -p tau-coding-agent gateway_contract -- --test-threads=1`
2. Validate runtime behavior coverage:
   `cargo test -p tau-coding-agent gateway_runtime -- --test-threads=1`
3. Run deterministic demo:
   `./scripts/demo/gateway.sh`
4. Verify transport health and status gate:
   `--transport-health-inspect gateway --transport-health-json`
   `--gateway-status-inspect --gateway-status-json`
5. Promote by increasing fixture complexity gradually while monitoring:
   `failure_streak`, `last_cycle_failed`, `last_retryable_failures`, `queue_depth`, `rollout_gate`, `rollout_reason_code`.

## Rollback plan

1. Stop invoking `--gateway-contract-runner`.
2. Preserve `.tau/gateway/` for incident analysis.
3. Revert to last known-good revision:
   `git revert <commit>`
4. Re-run validation matrix before re-enable.

## Troubleshooting

- Symptom: health state `degraded` with `case_processing_failed`.
  Action: inspect `runtime-events.jsonl`, then verify fixture expectations and schema compatibility.
- Symptom: health state `degraded` with `retry_attempted` or `retryable_failures_observed`.
  Action: inspect fixture retry cases and confirm transient failure semantics.
- Symptom: health state `failing` (`failure_streak >= 3`).
  Action: treat as rollout gate failure; pause promotion and investigate repeated failures.
- Symptom: `rollout_gate=hold` with low event volume.
  Action: run deterministic demo to refresh state and verify signal freshness.
- Symptom: non-zero `queue_depth`.
  Action: reduce per-cycle fixture volume or split fixture runs into smaller batches.
