# Issue 1671 Plan

Status: Reviewed

## Approach

1. Add a safety reward policy model in `tau-training-runner` with:
   - deterministic reason-code penalty mapping
   - hard-gate reason-code matching
   - policy validation for finite/non-negative numeric constraints
2. Integrate shaping into `TauAgentExecutor::execute` by deriving safety signals
   from tracer spans and appending/clamping rewards accordingly.
3. Extend prompt-optimization config parsing in `tau-coding-agent` with an
   optional `safety_reward` block and validate/apply overrides.
4. Add tests-first coverage in `tau-training-runner` and `tau-coding-agent`
   proving penalty mapping, hard-gate behavior, and config validation.
5. Update training operations docs to document `safety_reward` config fields.

## Affected Areas

- `crates/tau-training-runner/src/lib.rs`
- `crates/tau-coding-agent/src/training_runtime.rs`
- `docs/guides/training-ops.md`
- `specs/1671/spec.md`
- `specs/1671/plan.md`
- `specs/1671/tasks.md`

## Risks And Mitigations

- Risk: over-penalizing normal trajectories due to broad defaults.
  - Mitigation: keep defaults explicit/documented and allow config overrides.
- Risk: silent invalid config values (NaN/negative) causing undefined shaping.
  - Mitigation: strict policy validation with fail-closed runtime errors.
- Risk: gate logic drifting from safety event payload assumptions.
  - Mitigation: tests use realistic `agent.safety_policy_applied` span shapes.

## ADR

No dependency/protocol/schema migration requiring ADR; runtime config extension
is backward compatible and local to prompt-optimization config parsing.
