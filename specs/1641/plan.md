# Issue 1641 Plan

Status: Reviewed

## Approach

1. Tests-first:
   - add `scripts/dev/test-safety-fail-closed-contract.sh` to assert required
     source/test/docs contract markers for fail-closed bypass handling
   - add a regression test in `tau-agent-core` for outbound payload
     serialization failure under block mode (RED)
2. Implement fail-closed runtime behavior in
   `sanitize_outbound_http_request` for serialization failures when outbound
   block mode is active.
3. Update quickstart docs with explicit stage-by-stage fail-closed semantics
   and bypass expectations.
4. Run GREEN checks:
   - contract harness script
   - targeted regression/integration safety tests
5. Run scoped quality checks:
   - `scripts/dev/roadmap-status-sync.sh --check --quiet`
   - `cargo fmt --check`
   - `cargo clippy -p tau-agent-core -- -D warnings`

## Affected Areas

- `crates/tau-agent-core/src/lib.rs`
- `crates/tau-agent-core/src/tests/safety_pipeline.rs`
- `scripts/dev/test-safety-fail-closed-contract.sh`
- `docs/guides/quickstart.md`
- `specs/1641/spec.md`
- `specs/1641/plan.md`
- `specs/1641/tasks.md`

## Risks And Mitigations

- Risk: new fail-closed path alters behavior in non-block modes.
  - Mitigation: only enforce serialization failure as hard block when outbound
    secret-leak mode is `Block`; preserve existing behavior for warn/redact.
- Risk: docs drift from actual enforcement logic.
  - Mitigation: conformance harness validates required docs tokens and test
    coverage references.

## Interfaces / Contracts

- Runtime contract:
  - `sanitize_outbound_http_request` must return
    `AgentError::SafetyViolation { stage: "outbound_http_payload", ... }`
    when serialization fails and outbound block mode is active.
- Test contract:
  - regression test names and expected stage/reason code are stable and checked
    by harness.

## ADR

No dependency/protocol/architecture decision changes; ADR not required.
