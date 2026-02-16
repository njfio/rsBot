# Issue 1630 Plan

Status: Reviewed

## Approach

1. Add tests-first audit harness: `scripts/dev/test-contract-runner-remnants.sh`.
   - Validate M21 inventory posture for `tau-contract-runner-remnants`.
   - Validate removed runner flags are absent from non-test startup-dispatch code.
   - Validate demo scripts do not invoke removed runner flags.
   - Validate `docs/guides/transports.md` exposes a removed-runner migration matrix.
2. Run harness expecting RED on missing docs matrix.
3. Update `docs/guides/transports.md` with explicit removed contract-runner migration matrix.
4. Re-run harness for GREEN.
5. Run targeted integration regression tests:
   - `cargo test -p tau-onboarding regression_validate_transport_mode_cli_rejects_removed_memory_contract_runner`
   - `cargo test -p tau-onboarding regression_validate_transport_mode_cli_rejects_removed_browser_automation_contract_runner`
   - `cargo test -p tau-onboarding regression_validate_transport_mode_cli_rejects_removed_dashboard_contract_runner`
   - `cargo test -p tau-onboarding regression_validate_transport_mode_cli_rejects_removed_custom_command_contract_runner`
   - `cargo test -p tau-coding-agent unit_validate_memory_contract_runner_cli_is_removed`
6. Run scoped quality checks:
   - `scripts/dev/roadmap-status-sync.sh --check --quiet`
   - `cargo fmt --check`
   - `cargo clippy -p tau-onboarding -- -D warnings`

## Affected Areas

- `scripts/dev/test-contract-runner-remnants.sh`
- `docs/guides/transports.md`
- `specs/1630/spec.md`
- `specs/1630/plan.md`
- `specs/1630/tasks.md`

## Risks And Mitigations

- Risk: over-aggressive removal could break supported contract-runner modes.
  - Mitigation: audit only removed flags and run targeted validation tests.
- Risk: docs drift from runtime removed-flag behavior.
  - Mitigation: harness asserts explicit migration matrix text.

## ADR

No dependency/protocol/architecture decision changes; ADR not required.
