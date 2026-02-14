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

## Gateway dashboard backend API (schema v1)

When the gateway OpenResponses server is running, dashboard backend endpoints are available:

- `GET /dashboard/health`
- `GET /dashboard/widgets`
- `GET /dashboard/queue-timeline`
- `GET /dashboard/alerts`
- `POST /dashboard/actions` (`{"action":"pause|resume|refresh","reason":"..."}`)
- `GET /dashboard/stream` (SSE)

All dashboard endpoint payloads include `schema_version=1`.

Action endpoint side-effects:

- appends audit events to `.tau/dashboard/actions-audit.jsonl`
- updates `.tau/dashboard/control-state.json`
- affects control-plane gate semantics (`pause` => `rollout_gate=hold`)

Stream reconnect semantics:

- send `Last-Event-ID` header to request a reset handshake
- server emits `event: dashboard.reset`, then emits `event: dashboard.snapshot`

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

## Rollback plan

1. Stop invoking `--dashboard-contract-runner`.
2. Preserve `.tau/dashboard/` for incident analysis.
3. Revert to last known-good revision:
   `git revert <commit>`
4. Re-run validation matrix before re-enable.

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
