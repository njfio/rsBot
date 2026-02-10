# Gateway Operations Runbook

Run all commands from repository root.

## Scope

This runbook covers:

- fixture-driven gateway runtime (`--gateway-contract-runner`)
- gateway service lifecycle control (`--gateway-service-start|stop|status`)
- OpenResponses-compatible HTTP gateway (`--gateway-openresponses-server`)
- gateway-served webchat/control endpoints (`/webchat`, `/gateway/status`)

## Service lifecycle commands

Start service mode (persists lifecycle posture):

```bash
cargo run -p tau-coding-agent -- \
  --gateway-state-dir .tau/gateway \
  --gateway-service-start
```

Stop service mode with an explicit reason:

```bash
cargo run -p tau-coding-agent -- \
  --gateway-state-dir .tau/gateway \
  --gateway-service-stop \
  --gateway-service-stop-reason maintenance_window
```

Inspect lifecycle posture:

```bash
cargo run -p tau-coding-agent -- \
  --gateway-state-dir .tau/gateway \
  --gateway-service-status \
  --gateway-service-status-json
```

## OpenResponses endpoint (`/v1/responses`)

Start the authenticated OpenResponses endpoint:

```bash
cargo run -p tau-coding-agent -- \
  --model openai/gpt-4o-mini \
  --gateway-state-dir .tau/gateway \
  --gateway-openresponses-server \
  --gateway-openresponses-bind 127.0.0.1:8787 \
  --gateway-openresponses-auth-token local-dev-token \
  --gateway-openresponses-max-input-chars 32000
```

Non-stream request:

```bash
curl -sS http://127.0.0.1:8787/v1/responses \
  -H "Authorization: Bearer local-dev-token" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "openai/gpt-4o-mini",
    "instructions": "Be concise",
    "input": [
      {"type":"message","role":"user","content":[{"type":"input_text","text":"Summarize this system."}]}
    ]
  }'
```

SSE stream request:

```bash
curl -N http://127.0.0.1:8787/v1/responses \
  -H "Authorization: Bearer local-dev-token" \
  -H "Content-Type: application/json" \
  -d '{
    "input":"stream this response",
    "stream": true
  }'
```

Current compatibility notes:

- Supported input forms: `input` string, message item arrays, and `function_call_output` items.
- Session continuity derives from `metadata.session_id`, then `conversation`, then `previous_response_id`.
- Unknown request fields are ignored safely and surfaced in `ignored_fields` on the response.
- `model` in request payload is accepted but ignored; runtime uses CLI-selected model.

Webchat/control surface:

- Open browser at `http://127.0.0.1:8787/webchat`.
- Paste the same bearer token used for `/v1/responses` (`local-dev-token` in this example).
- Use the status refresh control to inspect `/gateway/status` from the same page.

Direct status endpoint check:

```bash
curl -sS http://127.0.0.1:8787/gateway/status \
  -H "Authorization: Bearer local-dev-token"
```

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
- `rollout_reason_code=service_stopped`: lifecycle stop posture is forcing rollout hold.

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
3. Start service lifecycle posture:
   `--gateway-service-start`
4. Run deterministic demo:
   `./scripts/demo/gateway.sh`
5. Verify transport health and status gate:
   `--transport-health-inspect gateway --transport-health-json`
   `--gateway-status-inspect --gateway-status-json`
6. Promote by increasing fixture complexity gradually while monitoring:
   `failure_streak`, `last_cycle_failed`, `last_retryable_failures`, `queue_depth`, `rollout_gate`, `rollout_reason_code`.

## Rollback plan

1. Stop service lifecycle posture:
   `--gateway-service-stop --gateway-service-stop-reason emergency_rollback`
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
