# Prompt Optimization Recovery Runbook

This runbook covers crash-safe resume behavior for prompt optimization lifecycle
control commands.

## Scope

- control action: `--prompt-optimization-control-resume`
- control state root: `--prompt-optimization-control-state-dir`
- artifacts:
  - `control-state.json`
  - `control-audit.jsonl`
  - `status.json`
  - `policy-checkpoint.json`
  - `policy-checkpoint.rollback.json`
  - `recovery-report.json`

## Resume Workflow

1. Validate the operator principal is authorized by RBAC policy.
2. Detect crash state from persisted lifecycle/training state.
3. Replay control-audit actions to deterministic recovery metadata.
4. If crash detected, restore checkpoint from:
   - primary: `policy-checkpoint.json`
   - fallback: `policy-checkpoint.rollback.json`
5. Persist `recovery-report.json`.
6. Persist updated `control-state.json` and append `control-audit.jsonl`.

## Standard Command

```bash
cargo run -p tau-coding-agent -- \
  --prompt-optimization-control-resume \
  --prompt-optimization-control-state-dir .tau/prompt-optimization \
  --prompt-optimization-control-rbac-policy .tau/rbac-policy.json \
  --prompt-optimization-control-principal local:rl-operator \
  --prompt-optimization-control-json
```

## Recovery Report Fields

`recovery-report.json` includes:

- `crash_detected`
- `prior_lifecycle_state`
- `prior_training_state`
- `replayed_audit_events`
- `replayed_actions`
- `checkpoint_source` (`primary` or `fallback`)
- `checkpoint_run_id`
- `checkpoint_global_step`
- `checkpoint_optimizer_step`
- `diagnostics`

## Failure Modes

### Crash detected and no usable checkpoint

Behavior: command fails closed with `resume recovery guardrail` error.

Operator action:

1. Restore a valid `policy-checkpoint.json` or `policy-checkpoint.rollback.json`.
2. Re-run resume command.

### Corrupted primary checkpoint

Behavior: fallback checkpoint is used and `diagnostics` records the primary
load failure.

Operator action:

1. Confirm `checkpoint_source=fallback` in `recovery-report.json`.
2. Replace corrupted primary checkpoint before next maintenance window.

### Malformed control audit rows

Behavior: command fails with line-indexed parse context from
`control-audit.jsonl`.

Operator action:

1. Repair invalid JSON row(s) in audit file.
2. Re-run resume command.

