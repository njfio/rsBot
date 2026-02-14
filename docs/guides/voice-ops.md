# Voice Operations Runbook

Run all commands from repository root.

## Scope

This runbook covers both voice runtime modes:

- Fixture-driven contract replay (`--voice-contract-runner`) for deterministic wake-word/turn
  contract validation.
- Fixture-driven live session replay (`--voice-live-runner`) for end-to-end live session proof,
  fallback handling, and artifact capture.

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

Live demo proof artifacts:

- `.tau/demo-voice-live/state.json`
- `.tau/demo-voice-live/runtime-events.jsonl`
- `.tau/demo-voice-live/channel-store/voice/<speaker_id>/...`
- `.tau/demo-voice-live/artifact-manifest.json`

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
- `frames_ignored_no_wake_word`
- `invalid_audio_frames_observed`
- `provider_outage_observed`
- `tts_output_emitted`

Guardrail interpretation:

- `rollout_gate=pass`: health is `healthy`, promotion can continue.
- `rollout_gate=hold`: health is `degraded` or `failing`, pause promotion and investigate.

## Deterministic demo path

```bash
./scripts/demo/voice.sh
./scripts/demo/voice-live.sh
```

`voice-live.sh` writes a machine-readable proof manifest at
`.tau/demo-voice-live/artifact-manifest.json` that records artifact paths and latest
health/reason-code snapshot.

## Rollout plan with guardrails

1. Validate fixture contract and runtime locally:
   `cargo test -p tau-coding-agent voice_contract -- --test-threads=1`
2. Validate runtime behavior coverage:
   `cargo test -p tau-coding-agent voice_runtime -- --test-threads=1`
3. Validate onboarding dispatch paths:
   `cargo test -p tau-onboarding integration_run_voice_contract_runner_if_requested_executes_runtime -- --test-threads=1`
   `cargo test -p tau-onboarding integration_run_voice_live_runner_if_requested_executes_runtime -- --test-threads=1`
4. Run deterministic demos:
   `./scripts/demo/voice.sh`
   `./scripts/demo/voice-live.sh`
5. Verify health and status gate:
   `--transport-health-inspect voice --transport-health-json`
   `--voice-status-inspect --voice-status-json`
6. Verify live-run proof manifest:
   `.tau/demo-voice-live/artifact-manifest.json`
7. Promote by increasing fixture complexity gradually while monitoring:
   `failure_streak`, `last_cycle_failed`, `queue_depth`, `rollout_gate`,
   `wake_word_detected`, `turns_handled`, `invalid_audio_frames_observed`,
   and `provider_outage_observed`.

## Rollback plan

1. Stop invoking `--voice-contract-runner` and `--voice-live-runner`.
2. Preserve `.tau/voice/` for incident analysis.
3. Revert to last known-good revision:
   `git revert <commit>`
4. Re-run validation matrix before re-enable.

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
- Symptom: live manifest missing expected artifacts.
  Action: rerun `./scripts/demo/voice-live.sh`, then verify `.tau/demo-voice-live/state.json` and
  `.tau/demo-voice-live/runtime-events.jsonl` exist before triaging runner output.
