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
- `command_run_timeout_observed`
- `command_run_non_zero_exit_observed`
- `command_run_spawn_failures_observed`
- `command_run_missing_command_observed`

Policy contract defaults (`--custom-command-contract-runner`):

- `--custom-command-policy-require-approval=true`
- `--custom-command-policy-allow-shell=false`
- `--custom-command-policy-sandbox-profile=restricted`
- `--custom-command-policy-allowed-env` optional allowlist
- `--custom-command-policy-denied-env` optional denylist override

By default, unsafe shell control operators in command templates are rejected
deterministically as malformed policy-denied definitions.

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

## Rollback plan

1. Stop invoking `--custom-command-contract-runner`.
2. Preserve `.tau/custom-command/` for incident analysis.
3. Revert to last known-good revision:
   `git revert <commit>`
4. Re-run validation matrix before re-enable.

## Troubleshooting

- Symptom: health state `degraded` with `case_processing_failed`.
  Action: inspect `runtime-events.jsonl`, then verify fixture expectations and schema compatibility.
- Symptom: repeated `retry_attempted` or `retryable_failures_observed`.
  Action: confirm transient failure semantics and retry configuration.
- Symptom: rollout hold with `command_registry_mutated` missing when expecting changes.
  Action: confirm fixture operations include `create`, `update`, or `delete`.
- Symptom: high duplicate counts.
  Action: inspect processed-case keys and verify fixture case identifiers are unique.
