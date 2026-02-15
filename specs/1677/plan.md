# Issue 1677 Plan

Status: Reviewed

## Approach

1. Extend `execute_prompt_optimization_control_command` resume path in
   `tau-coding-agent::training_runtime` with deterministic recovery workflow:
   - detect interrupted state from control/status artifacts
   - replay control audit rows into recovery summary
   - load checkpoints via `load_policy_checkpoint_with_rollback` using
     state-dir paths (`policy-checkpoint.json` + `policy-checkpoint.rollback.json`)
   - enforce guardrails for crash-detected resume when checkpoints are missing
2. Persist `recovery-report.json` with stable schema fields for operators.
3. Keep lifecycle action auth enforcement unchanged via `tau-access`.
4. Add tests for crash detection, fallback restore, and fail-closed guardrails.
5. Add a concise operator runbook under docs for recovery procedure.

## Affected Areas

- `crates/tau-coding-agent/src/training_runtime.rs`
- `crates/tau-coding-agent/src/tests/auth_provider/runtime_and_startup/startup_preflight_and_policy.rs` (if needed for preflight-level recovery assertions)
- `docs/guides/prompt-optimization-recovery-runbook.md` (new)
- `specs/1677/spec.md`
- `specs/1677/plan.md`
- `specs/1677/tasks.md`

## Risks And Mitigations

- Risk: false-positive crash detection causes unnecessary recovery failures.
  - Mitigation: deterministic rule set + explicit tests for interrupted vs safe states.
- Risk: corrupted checkpoint payloads hide root cause.
  - Mitigation: propagate checkpoint loader diagnostics into recovery report/errors.
- Risk: resume behavior drift in future refactors.
  - Mitigation: conformance tests tied to schema and fallback semantics.

## ADR

No architecture dependency/protocol change; ADR not required.
