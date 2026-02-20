# Gateway Operations Runbook

Run all commands from repository root.

## Scope

This runbook covers:

- fixture-driven gateway runtime (`--gateway-contract-runner`)
- gateway service lifecycle control (`--gateway-service-start|stop|status`)
- OpenResponses-compatible HTTP gateway (`--gateway-openresponses-server`)
- gateway-served webchat/control endpoints (`/webchat`, `/gateway/status`)
- optional OpenTelemetry-compatible JSON export (`--otel-export-log`)

Canonical promotion gate procedure:

- `docs/guides/ops-readiness-live-validation.md`

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

## OpenTelemetry export path

Enable OpenTelemetry-compatible export records for runtime/gateway observability:

```bash
cargo run -p tau-coding-agent -- \
  --gateway-contract-runner \
  --gateway-state-dir .tau/gateway \
  --otel-export-log .tau/observability/otel-export.jsonl
```

Export stream shape (JSONL, additive to existing runtime logs):

- `record_type=otel_export_v1`
- `schema_version=1`
- `signal=trace|metric`
- `resource.service.name=tau-runtime|tau-gateway`

## OpenResponses endpoint (`/v1/responses`) and OpenAI-compatible adapters

Start the OpenResponses endpoint in `token` auth mode:

```bash
cargo run -p tau-coding-agent -- \
  --model openai/gpt-4o-mini \
  --gateway-state-dir .tau/gateway \
  --gateway-openresponses-server \
  --gateway-openresponses-bind 127.0.0.1:8787 \
  --gateway-openresponses-auth-mode token \
  --gateway-openresponses-auth-token local-dev-token \
  --gateway-openresponses-max-input-chars 32000
```

Preferred encrypted-secret workflow (credential-store IDs):

```bash
cargo run -p tau-coding-agent -- \
  --integration-auth "/integration-auth set gateway-openresponses-auth-token local-dev-token"

cargo run -p tau-coding-agent -- \
  --model openai/gpt-4o-mini \
  --gateway-state-dir .tau/gateway \
  --gateway-openresponses-server \
  --gateway-openresponses-bind 127.0.0.1:8787 \
  --gateway-openresponses-auth-mode token \
  --gateway-openresponses-auth-token-id gateway-openresponses-auth-token \
  --gateway-openresponses-max-input-chars 32000
```

Remote-access profile posture:

- `--gateway-remote-profile local-only` (default)
- `--gateway-remote-profile password-remote`
- `--gateway-remote-profile proxy-remote`
- `--gateway-remote-profile tailscale-serve`
- `--gateway-remote-profile tailscale-funnel`

Inspect remote-access posture without starting the gateway:

```bash
cargo run -p tau-coding-agent -- \
  --gateway-remote-profile-inspect \
  --gateway-remote-profile-json
```

Export remote workflow plans (fails closed when selected profile is unsafe):

```bash
cargo run -p tau-coding-agent -- \
  --gateway-remote-plan \
  --gateway-remote-plan-json
```

Detailed setup and rollback guidance:

```bash
cat docs/guides/gateway-remote-access.md
```

Auth mode summary:

- `token` (default): bearer token required on `/v1/responses`, `/v1/chat/completions`, `/v1/completions`, `/v1/models`, and `/gateway/status` (set direct token or `--gateway-openresponses-auth-token-id`).
- `password-session`: exchange password once at `/gateway/auth/session`, then use returned bearer session token (set direct password or `--gateway-openresponses-auth-password-id`).
- `localhost-dev`: no bearer required, but bind must be loopback (`127.0.0.1`/`::1`).

Password-session startup example:

```bash
cargo run -p tau-coding-agent -- \
  --model openai/gpt-4o-mini \
  --gateway-state-dir .tau/gateway \
  --gateway-openresponses-server \
  --gateway-openresponses-bind 127.0.0.1:8787 \
  --gateway-openresponses-auth-mode password-session \
  --gateway-openresponses-auth-password "local-password" \
  --gateway-openresponses-session-ttl-seconds 3600 \
  --gateway-openresponses-rate-limit-window-seconds 60 \
  --gateway-openresponses-rate-limit-max-requests 120
```

Password-session via credential-store ID:

```bash
cargo run -p tau-coding-agent -- \
  --integration-auth "/integration-auth set gateway-openresponses-auth-password local-password"

cargo run -p tau-coding-agent -- \
  --model openai/gpt-4o-mini \
  --gateway-state-dir .tau/gateway \
  --gateway-openresponses-server \
  --gateway-openresponses-bind 127.0.0.1:8787 \
  --gateway-openresponses-auth-mode password-session \
  --gateway-openresponses-auth-password-id gateway-openresponses-auth-password \
  --gateway-openresponses-session-ttl-seconds 3600 \
  --gateway-openresponses-rate-limit-window-seconds 60 \
  --gateway-openresponses-rate-limit-max-requests 120
```

Issue a bearer session token (password-session mode only):

```bash
curl -sS http://127.0.0.1:8787/gateway/auth/session \
  -H "Content-Type: application/json" \
  -d '{"password":"local-password"}'
```

Expected response includes:

- `access_token` (use as bearer token)
- `token_type` (`bearer`)
- `expires_unix_ms`
- `expires_in_seconds`

Non-stream request (token or password-session):

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

OpenAI-compatible non-stream chat request:

```bash
curl -sS http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer local-dev-token" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "openai/gpt-4o-mini",
    "messages": [{"role":"user","content":"Say hi in one sentence."}]
  }'
```

OpenAI-compatible streaming completion request:

```bash
curl -N http://127.0.0.1:8787/v1/completions \
  -H "Authorization: Bearer local-dev-token" \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "Generate a one-line release note.",
    "stream": true
  }'
```

OpenAI-compatible model listing:

```bash
curl -sS http://127.0.0.1:8787/v1/models \
  -H "Authorization: Bearer local-dev-token"
```

Current compatibility notes:

- Supported input forms: `input` string, message item arrays, and `function_call_output` items.
- Session continuity derives from `metadata.session_id`, then `conversation`, then `previous_response_id`.
- Unknown request fields are ignored safely and surfaced in `ignored_fields` on the response.
- `model` in request payload is accepted but ignored; runtime uses CLI-selected model.
- OpenAI-compatible adapters reuse the same auth/rate-limit/session semantics as `/v1/responses`.

Webchat/control surface:

- Open browser at `http://127.0.0.1:8787/webchat`.
- Token mode: paste the configured bearer token (`local-dev-token` in this example).
- Password-session mode: first issue a session token from `/gateway/auth/session`, then paste that token.
- Localhost-dev mode: token can be left empty.
- Use the multi-view tabs for:
  - `Conversation` (OpenResponses + OpenAI-compatible send/stream)
  - `Tools` (status/connector/reason-code diagnostics)
  - `Sessions` (browse/detail/append/reset with policy gate)
  - `Memory` (session-scoped memory note read/write with policy gate)
  - `Configuration` (runtime endpoint/gate summary)

Direct status endpoint check:

```bash
curl -sS http://127.0.0.1:8787/gateway/status \
  -H "Authorization: Bearer local-dev-token"
```

`/gateway/status` response includes an `auth` block with:

- `mode`
- `session_ttl_seconds`
- `active_sessions`
- `total_sessions_issued`
- `auth_failures`
- `rate_limited_requests`
- `rate_limit_window_seconds`
- `rate_limit_max_requests`

`/gateway/status` includes `gateway.openai_compat` with:

- endpoint paths (`chat_completions_endpoint`, `completions_endpoint`, `models_endpoint`)
- runtime counters (`total_requests`, `chat_completions_requests`, `completions_requests`, `models_requests`, `stream_requests`)
- diagnostics (`translation_failures`, `execution_failures`, `reason_code_counts`, `ignored_field_counts`, `last_reason_codes`)

`/gateway/status` includes `gateway.web_ui` with:

- endpoint paths (`sessions_endpoint`, `session_detail_endpoint`, `session_append_endpoint`, `session_reset_endpoint`, `memory_endpoint`, `ui_telemetry_endpoint`, `cortex_chat_endpoint`, `cortex_status_endpoint`)
- policy gates (`policy_gates.session_write`, `policy_gates.memory_write`)
- UI telemetry runtime counters (`telemetry_runtime.total_events`, `last_event_unix_ms`, `view_counts`, `action_counts`, `reason_code_counts`)

Cortex readiness live validation:

```bash
TAU_CORTEX_AUTH_TOKEN=local-dev-token \
scripts/dev/cortex-readiness-live-check.sh \
  --base-url http://127.0.0.1:8787 \
  --expect-health-state healthy
```

`/cortex/status` readiness fields include:

- `health_state` (`healthy|degraded|failing|unknown`)
- `rollout_gate` (`pass|hold`)
- `reason_code`
- `health_reason`
- `last_event_unix_ms`
- `last_event_age_seconds`

Common readiness reason codes:

- `cortex_ready`
- `cortex_chat_activity_missing`
- `cortex_observer_events_stale`
- `cortex_observer_events_empty`
- `cortex_observer_events_malformed`
- `cortex_observer_events_missing`
- `cortex_observer_events_read_failed`

Session admin API examples:

```bash
curl -sS http://127.0.0.1:8787/gateway/sessions \
  -H "Authorization: Bearer local-dev-token"

curl -sS http://127.0.0.1:8787/gateway/sessions/default/append \
  -H "Authorization: Bearer local-dev-token" \
  -H "Content-Type: application/json" \
  -d '{"role":"user","content":"manual session note","policy_gate":"allow_session_write"}'

curl -sS http://127.0.0.1:8787/gateway/sessions/default/reset \
  -H "Authorization: Bearer local-dev-token" \
  -H "Content-Type: application/json" \
  -d '{"policy_gate":"allow_session_write"}'
```

Memory admin API examples:

```bash
curl -sS http://127.0.0.1:8787/gateway/memory/default \
  -H "Authorization: Bearer local-dev-token"

curl -sS -X PUT http://127.0.0.1:8787/gateway/memory/default \
  -H "Authorization: Bearer local-dev-token" \
  -H "Content-Type: application/json" \
  -d '{"content":"operator memory note","policy_gate":"allow_memory_write"}'
```

`/gateway/status` also includes a `runtime_heartbeat` block with:

- `run_state` (`running|stopped|disabled|unknown`)
- `reason_code`
- `interval_ms`
- `tick_count`
- `queue_depth`
- `pending_events`
- `pending_jobs`
- `temp_files_cleaned`
- `stuck_jobs`
- `stuck_tool_builds`
- `repair_actions`
- `retries_queued`
- `retries_exhausted`
- `orphan_artifacts_cleaned`
- `reason_codes[]`
- `diagnostics[]`

`/gateway/status` includes an `events` block for routine scheduler posture:

- `health_state`, `rollout_gate`, `reason_code`, `health_reason`
- definition counters (`discovered_events`, `enabled_events`, `malformed_events`)
- queue posture (`due_now_events`, `queued_now_events`, `not_due_events`)
- execution history (`execution_history_entries`, `executed_history_entries`, `failed_history_entries`, `skipped_history_entries`, `last_execution_unix_ms`, `last_execution_reason_code`)
- `diagnostics[]`

## Runtime heartbeat scheduler

Enable/disable and tune scheduler cadence:

```bash
cargo run -p tau-coding-agent -- \
  --gateway-openresponses-server \
  --gateway-openresponses-auth-mode token \
  --gateway-openresponses-auth-token local-dev-token \
  --runtime-heartbeat-enabled=true \
  --runtime-heartbeat-interval-ms 5000 \
  --runtime-self-repair-enabled=true \
  --runtime-self-repair-timeout-ms 60000 \
  --runtime-self-repair-max-retries 2 \
  --runtime-self-repair-tool-builds-dir .tau/tool-builds \
  --runtime-self-repair-orphan-max-age-seconds 3600
```

By default, gateway mode writes heartbeat diagnostics to:

- `.tau/gateway/runtime-heartbeat/state.json`
- `.tau/gateway/runtime-heartbeat/runtime-heartbeat-events.jsonl`

Override state snapshot path explicitly:

```bash
cargo run -p tau-coding-agent -- \
  --gateway-openresponses-server \
  --gateway-openresponses-auth-mode token \
  --gateway-openresponses-auth-token local-dev-token \
  --runtime-heartbeat-state-path .tau/runtime-heartbeat/state.json
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
./scripts/demo/gateway-remote-access.sh
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
