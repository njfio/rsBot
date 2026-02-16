# Issue 1643 Plan

Status: Reviewed

## Approach

1. Tests-first:
   - add `scripts/dev/test-outbound-safety-enforcement.sh` to assert required
     outbound source/test/docs contract markers
   - run harness for RED against missing quickstart outbound validation section
2. Update quickstart with deterministic outbound validation commands.
3. Run harness for GREEN.
4. Run targeted outbound safety tests:
   - `cargo test -p tau-agent-core integration_secret_leak_policy_blocks_outbound_http_payload`
   - `cargo test -p tau-agent-core functional_secret_leak_policy_redacts_outbound_http_payload`
   - `cargo test -p tau-agent-core integration_outbound_secret_fixture_matrix_blocks_all_cases`
   - `cargo test -p tau-agent-core functional_outbound_secret_fixture_matrix_redacts_all_cases`
   - `cargo test -p tau-agent-core regression_outbound_secret_fixture_matrix_reason_codes_are_stable`
   - `cargo test -p tau-agent-core regression_secret_leak_block_fails_closed_when_outbound_payload_serialization_fails`
5. Run scoped quality checks:
   - `scripts/dev/roadmap-status-sync.sh --check --quiet`
   - `cargo fmt --check`
   - `cargo clippy -p tau-agent-core -- -D warnings`

## Affected Areas

- `scripts/dev/test-outbound-safety-enforcement.sh`
- `docs/guides/quickstart.md`
- `specs/1643/spec.md`
- `specs/1643/plan.md`
- `specs/1643/tasks.md`

## Risks And Mitigations

- Risk: parent closure drifts from actual outbound test coverage.
  - Mitigation: harness checks concrete outbound test identifiers and docs
    command tokens.
- Risk: docs examples go stale.
  - Mitigation: keep commands aligned with harness checks for deterministic
    operator validation.

## Interfaces / Contracts

- Harness contract:
  - outbound fixture matrix include and targeted test identifiers must exist.
  - quickstart must include outbound validation heading + command tokens.
- Verification contract:
  - listed outbound test commands and scoped checks must pass.

## ADR

No dependency/protocol/architecture decision changes; ADR not required.
