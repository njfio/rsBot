# Custom Command Operations Runbook

Run all commands from repository root.

## Scope

This runbook covers the fixture-driven no-code custom command runtime
(`--custom-command-contract-runner`).

## Health and observability signals

Primary transport health signal:

```bash
cargo run -p tau-coding-agent -- \
  --custom-command-state-dir .tau/custom-command \
  --transport-health-inspect custom-command \
  --transport-health-json
```

Primary operator status/guardrail signal:

```bash
cargo run -p tau-coding-agent -- \
  --custom-command-state-dir .tau/custom-command \
  --custom-command-status-inspect \
  --custom-command-status-json
```

Primary state files:

- `.tau/custom-command/state.json`
- `.tau/custom-command/runtime-events.jsonl`
- `.tau/custom-command/channel-store/custom-command/<command_name or registry>/...`

`runtime-events.jsonl` reason codes:

- `healthy_cycle`
- `queue_backpressure_applied`
- `duplicate_cases_skipped`
- `malformed_inputs_observed`
- `retry_attempted`
- `retryable_failures_observed`
- `case_processing_failed`
- `command_registry_mutated`
- `command_runs_recorded`

Guardrail interpretation:

- `rollout_gate=pass`: health is `healthy`, promotion can continue.
- `rollout_gate=hold`: health is `degraded` or `failing`, pause promotion and investigate.

## Deterministic demo path

```bash
./scripts/demo/custom-command.sh
```

## Rollout plan with guardrails

1. Validate contract fixtures and compatibility:
   `cargo test -p tau-coding-agent custom_command_contract -- --test-threads=1`
2. Validate runtime behavior coverage:
   `cargo test -p tau-coding-agent custom_command_runtime -- --test-threads=1`
3. Run deterministic demo:
   `./scripts/demo/custom-command.sh`
4. Verify transport health and status gate:
   `--transport-health-inspect custom-command --transport-health-json`
   `--custom-command-status-inspect --custom-command-status-json`
5. Promote by increasing fixture complexity while monitoring:
   `failure_streak`, `last_cycle_failed`, `queue_depth`, `rollout_gate`,
   `reason_code_counts`.

## Canary rollout profile

Apply the global rollout contract in [Release Channel Ops](release-channel-ops.md#cross-surface-rollout-contract).

| Phase | Canary % | Custom-command-specific gates |
| --- | --- | --- |
| canary-1 | 5% | `rollout_gate=pass`, `health_state=healthy`, `failure_streak=0`, `queue_depth<=1`, no new `case_processing_failed`. |
| canary-2 | 25% | canary-1 gates hold for 60 minutes; `command_runs_recorded` continues to advance. |
| canary-3 | 50% | canary-2 gates hold for 120 minutes; `command_registry_mutated` is present when registry changes are expected. |
| general-availability | 100% | 24-hour monitor window passes and release sign-off checklist is complete. |

## Rollback plan

1. Stop invoking `--custom-command-contract-runner`.
2. Preserve `.tau/custom-command/` for incident analysis.
3. Revert to last known-good revision:
   `git revert <commit>`
4. Re-run validation matrix before re-enable.
5. If any rollback trigger from [Rollback Trigger Matrix](release-channel-ops.md#rollback-trigger-matrix) fires, stop promotion immediately and execute [Rollback Execution Steps](release-channel-ops.md#rollback-execution-steps).

## Troubleshooting

- Symptom: health state `degraded` with `case_processing_failed`.
  Action: inspect `runtime-events.jsonl`, then verify fixture expectations and schema compatibility.
- Symptom: repeated `retry_attempted` or `retryable_failures_observed`.
  Action: confirm transient failure semantics and retry configuration.
- Symptom: rollout hold with `command_registry_mutated` missing when expecting changes.
  Action: confirm fixture operations include `create`, `update`, or `delete`.
- Symptom: high duplicate counts.
  Action: inspect processed-case keys and verify fixture case identifiers are unique.
