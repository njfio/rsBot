# Issue 1640 Plan

Status: Reviewed

## Approach

1. Tests-first: add `scripts/dev/test-startup-safety-policy-precedence.sh` that checks:
   - precedence constant + resolver wiring in `startup_safety_policy.rs`
   - explicit precedence section in `docs/guides/startup-di-pipeline.md`
2. Run harness for RED (expected docs section missing).
3. Update startup DI guide with explicit safety-policy precedence section.
4. Run harness for GREEN.
5. Run targeted tau-startup precedence tests:
   - `cargo test -p tau-startup unit_startup_safety_policy_precedence_layers_match_contract`
   - `cargo test -p tau-startup functional_resolve_startup_safety_policy_cli_flag_overrides_env_and_preset`
   - `cargo test -p tau-startup regression_resolve_startup_safety_policy_env_overrides_preset_when_cli_flag_unset`
6. Run scoped checks:
   - `scripts/dev/roadmap-status-sync.sh --check --quiet`
   - `cargo fmt --check`
   - `cargo clippy -p tau-startup -- -D warnings`

## Affected Areas

- `scripts/dev/test-startup-safety-policy-precedence.sh`
- `docs/guides/startup-di-pipeline.md`
- `specs/1640/spec.md`
- `specs/1640/plan.md`
- `specs/1640/tasks.md`

## Risks And Mitigations

- Risk: docs wording drifts from code precedence contract.
  - Mitigation: harness checks both source contract tokens and docs layer ordering.
- Risk: precedence behavior regresses during unrelated startup edits.
  - Mitigation: targeted regression tests in tau-startup are part of issue evidence.

## ADR

No dependency/protocol/architecture decision changes; ADR not required.
