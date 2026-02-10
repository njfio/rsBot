# Multi-Agent Operations Runbook

Run all commands from repository root.

## Scope

This runbook covers the fixture-driven multi-agent runtime (`--multi-agent-contract-runner`).

## Health and observability signals

Primary transport health signal:

```bash
cargo run -p tau-coding-agent -- \
  --multi-agent-state-dir .tau/multi-agent \
  --transport-health-inspect multi-agent \
  --transport-health-json
```

Primary operator status/guardrail signal:

```bash
cargo run -p tau-coding-agent -- \
  --multi-agent-state-dir .tau/multi-agent \
  --multi-agent-status-inspect \
  --multi-agent-status-json
```

Primary state files:

- `.tau/multi-agent/state.json`
- `.tau/multi-agent/runtime-events.jsonl`
- `.tau/multi-agent/channel-store/multi-agent/orchestrator-router/...`

`runtime-events.jsonl` reason codes:

- `healthy_cycle`
- `queue_backpressure_applied`
- `duplicate_cases_skipped`
- `malformed_inputs_observed`
- `retry_attempted`
- `retryable_failures_observed`
- `case_processing_failed`
- `routed_cases_updated`

Guardrail interpretation:

- `rollout_gate=pass`: health is `healthy`, promotion can continue.
- `rollout_gate=hold`: health is `degraded` or `failing`, pause promotion and investigate.

## Deterministic demo path

```bash
./scripts/demo/multi-agent.sh
```

## Rollout plan with guardrails

1. Validate fixture contract and runtime locally:
   `cargo test -p tau-coding-agent multi_agent_contract -- --test-threads=1`
2. Validate runtime behavior coverage:
   `cargo test -p tau-coding-agent multi_agent_runtime -- --test-threads=1`
3. Run deterministic demo:
   `./scripts/demo/multi-agent.sh`
4. Verify transport health and status gate:
   `--transport-health-inspect multi-agent --transport-health-json`
   `--multi-agent-status-inspect --multi-agent-status-json`
5. Promote by increasing fixture complexity gradually while monitoring:
   `failure_streak`, `last_cycle_failed`, `queue_depth`, `rollout_gate`.

## Rollback plan

1. Stop invoking `--multi-agent-contract-runner`.
2. Preserve `.tau/multi-agent/` for incident analysis.
3. Revert to last known-good revision:
   `git revert <commit>`
4. Re-run validation matrix before re-enable.

## Troubleshooting

- Symptom: health state `degraded` with `case_processing_failed`.
  Action: inspect `runtime-events.jsonl`, then verify fixture expectations and route-table integrity.
- Symptom: health state `degraded` with `retry_attempted` or `retryable_failures_observed`.
  Action: inspect fixture retry scenarios and adjust retry controls only for transient failures.
- Symptom: health state `failing` (`failure_streak >= 3`).
  Action: treat as rollout gate failure; pause promotion and investigate repeated failures.
- Symptom: `rollout_gate=hold` with low event volume.
  Action: run deterministic demo to refresh runtime state and confirm signal freshness.
- Symptom: non-zero `queue_depth`.
  Action: increase `--multi-agent-queue-limit` or reduce fixture batch size to avoid backlog drops.
