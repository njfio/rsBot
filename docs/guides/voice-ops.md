# Voice Operations Runbook

Run all commands from repository root.

## Scope

This runbook covers the fixture-driven voice runtime (`--voice-contract-runner`) for wake-word
detection and turn handling.

## Health and observability signals

Primary transport health signal:

```bash
cargo run -p tau-coding-agent -- \
  --voice-state-dir .tau/voice \
  --transport-health-inspect voice \
  --transport-health-json
```

Primary operator status/guardrail signal:

```bash
cargo run -p tau-coding-agent -- \
  --voice-state-dir .tau/voice \
  --voice-status-inspect \
  --voice-status-json
```

Primary state files:

- `.tau/voice/state.json`
- `.tau/voice/runtime-events.jsonl`
- `.tau/voice/channel-store/voice/<speaker_id>/...`

`runtime-events.jsonl` reason codes:

- `healthy_cycle`
- `queue_backpressure_applied`
- `duplicate_cases_skipped`
- `malformed_inputs_observed`
- `retry_attempted`
- `retryable_failures_observed`
- `case_processing_failed`
- `wake_word_detected`
- `turns_handled`

Guardrail interpretation:

- `rollout_gate=pass`: health is `healthy`, promotion can continue.
- `rollout_gate=hold`: health is `degraded` or `failing`, pause promotion and investigate.

## Deterministic demo path

```bash
./scripts/demo/voice.sh
```

## Rollout plan with guardrails

1. Validate fixture contract and runtime locally:
   `cargo test -p tau-coding-agent voice_contract -- --test-threads=1`
2. Validate runtime behavior coverage:
   `cargo test -p tau-coding-agent voice_runtime -- --test-threads=1`
3. Run deterministic demo:
   `./scripts/demo/voice.sh`
4. Verify health and status gate:
   `--transport-health-inspect voice --transport-health-json`
   `--voice-status-inspect --voice-status-json`
5. Promote by increasing fixture complexity gradually while monitoring:
   `failure_streak`, `last_cycle_failed`, `queue_depth`, `rollout_gate`,
   `wake_word_detected`, and `turns_handled`.

## Canary rollout profile

Apply the global rollout contract in [Release Channel Ops](release-channel-ops.md#cross-surface-rollout-contract).

| Phase | Canary % | Voice-specific gates |
| --- | --- | --- |
| canary-1 | 5% | `rollout_gate=pass`, `health_state=healthy`, `failure_streak=0`, `queue_depth<=1`, no new `case_processing_failed`. |
| canary-2 | 25% | canary-1 gates hold for 60 minutes; `wake_word_detected` and `turns_handled` continue to advance. |
| canary-3 | 50% | canary-2 gates hold for 120 minutes; `retryable_failures_observed` remains flat. |
| general-availability | 100% | 24-hour monitor window passes and release sign-off checklist is complete. |

## Rollback plan

1. Stop invoking `--voice-contract-runner`.
2. Preserve `.tau/voice/` for incident analysis.
3. Revert to last known-good revision:
   `git revert <commit>`
4. Re-run validation matrix before re-enable.
5. If any rollback trigger from [Rollback Trigger Matrix](release-channel-ops.md#rollback-trigger-matrix) fires, stop promotion immediately and execute [Rollback Execution Steps](release-channel-ops.md#rollback-execution-steps).

## Troubleshooting

- Symptom: health state `degraded` with `case_processing_failed`.
  Action: inspect `runtime-events.jsonl`, then validate fixture schema and expected payloads.
- Symptom: health state `degraded` with `malformed_inputs_observed`.
  Action: inspect transcript, wake-word, and locale fields for malformed fixture cases.
- Symptom: health state `degraded` with `retry_attempted` or `retryable_failures_observed`.
  Action: verify transient failure simulation and retry policy settings.
- Symptom: health state `failing` (`failure_streak >= 3`).
  Action: treat as rollout gate failure; pause promotion and investigate repeated failures.
- Symptom: `rollout_gate=hold` with stale state.
  Action: run deterministic demo and re-check `voice-status-inspect` freshness fields.
- Symptom: non-zero `queue_depth`.
  Action: reduce per-cycle fixture volume or increase `--voice-queue-limit`.
