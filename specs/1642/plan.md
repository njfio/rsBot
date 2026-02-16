# Issue 1642 Plan

Status: Reviewed

## Approach

1. Tests-first:
   - add `scripts/dev/test-inbound-tool-output-safety-enforcement.sh` to check
     required inbound/tool-output coverage markers in source/tests/docs
   - run harness for RED against missing docs section
2. Add operator-facing docs section in quickstart for deterministic inbound and
   tool-output validation commands.
3. Run harness for GREEN.
4. Execute targeted integration/regression safety tests:
   - `cargo test -p tau-agent-core functional_inbound_safety_fixture_corpus_applies_warn_and_redact_modes`
   - `cargo test -p tau-agent-core integration_inbound_safety_fixture_corpus_blocks_malicious_cases`
   - `cargo test -p tau-agent-core regression_inbound_safety_fixture_corpus_has_no_silent_pass_through_in_block_mode`
   - `cargo test -p tau-agent-core integration_tool_output_reinjection_fixture_suite_blocks_fail_closed`
   - `cargo test -p tau-agent-core regression_tool_output_reinjection_fixture_suite_emits_stable_stage_reason_codes`
5. Run scoped quality checks:
   - `scripts/dev/roadmap-status-sync.sh --check --quiet`
   - `cargo fmt --check`
   - `cargo clippy -p tau-agent-core -- -D warnings`

## Affected Areas

- `scripts/dev/test-inbound-tool-output-safety-enforcement.sh`
- `docs/guides/quickstart.md`
- `specs/1642/spec.md`
- `specs/1642/plan.md`
- `specs/1642/tasks.md`

## Risks And Mitigations

- Risk: parent closure drifts from actual test coverage.
  - Mitigation: harness validates concrete test identifiers in source.
- Risk: docs go stale as tests evolve.
  - Mitigation: docs token checks live in issue harness for future regressions.

## Interfaces / Contracts

- Conformance harness contract:
  - must require inbound corpus tests, tool-output reinjection regressions, and
    quickstart validation section tokens.
- Verification contract:
  - targeted commands listed above must pass in issue evidence.

## ADR

No dependency/protocol/architecture decision changes; ADR not required.
