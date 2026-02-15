# Issue 1676 Plan

Status: Reviewed

## Approach

1. Add explicit lifecycle control CLI arguments under prompt-optimization
   namespace (`status|pause|resume|cancel|rollback`) with state/audit path and
   auth context options.
2. Extend startup preflight action contracts (`tau-startup` +
   `tau-onboarding`) with a new callback for prompt-optimization control
   commands.
3. Implement control command runtime in `tau-coding-agent::training_runtime`:
   - command selection + validation (exactly one action)
   - RBAC enforcement through `tau-access::enforce_rl_lifecycle_action_with_policy_path`
   - deterministic control-state persistence
   - JSONL audit append per command
   - rollback checkpoint validation via trainer checkpoint loader
4. Add tests in CLI parsing/validation and startup/runtime integration for
   conformance C-01..C-05.
5. Run scoped format/lint/tests for touched crates.

## Affected Areas

- `crates/tau-cli/src/cli_args.rs`
- `crates/tau-startup/src/lib.rs`
- `crates/tau-onboarding/src/startup_preflight.rs`
- `crates/tau-coding-agent/src/startup_preflight.rs`
- `crates/tau-coding-agent/src/training_runtime.rs`
- `crates/tau-coding-agent/src/main.rs`
- `crates/tau-coding-agent/src/tests.rs`
- `crates/tau-coding-agent/src/tests/cli_validation.rs`
- `crates/tau-coding-agent/src/tests/auth_provider/runtime_and_startup/startup_preflight_and_policy.rs`
- `specs/1676/spec.md`
- `specs/1676/plan.md`
- `specs/1676/tasks.md`

## Risks And Mitigations

- Risk: command-mode conflicts with existing runtime flags.
  - Mitigation: explicit single-action validation and preflight short-circuit.
- Risk: audit/state drift from non-atomic writes.
  - Mitigation: deterministic write order (state first, audit append) with
    strict error propagation.
- Risk: rollback command accepts corrupted checkpoints.
  - Mitigation: validate rollback path with checkpoint loader before state write.

## ADR

No architectural dependency or protocol schema change; ADR not required.
