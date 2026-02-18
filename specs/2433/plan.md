# Plan #2433

Status: Reviewed
Spec: specs/2433/spec.md

## Approach

1. Capture and preserve RED evidence from the current failing tests.
2. Inspect bash tool policy metadata assembly and rate-limit decision path.
3. Apply minimal fixes to:
   - consistently populate policy metadata fields
   - enforce and persist throttle state per principal within window boundaries
4. Re-run targeted conformance tests, then crate-level bash subset checks.
5. Run fmt + clippy gate checks.

## Affected Modules (planned)

- `crates/tau-tools/src/tools.rs`
- `crates/tau-tools/src/tools/tests.rs`
- `specs/milestones/m73/index.md`
- `specs/2433/spec.md`
- `specs/2433/plan.md`
- `specs/2433/tasks.md`

## Risks and Mitigations

- Risk: tightening rate-limit logic may break unrelated principal flows.
  - Mitigation: explicitly verify cross-principal isolation and reset-window
    behavior in conformance tests.
- Risk: policy metadata fixes alter output shape unexpectedly.
  - Mitigation: keep output keys stable and only populate missing expected
    values.
- Risk: race/clock sensitivity in tests.
  - Mitigation: use deterministic test windows and assertions around observable
    deny/allow outcomes.
