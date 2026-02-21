# Plan: Issue #3008 - tau-diagnostics boundary test uplift

## Approach
1. Add focused RED tests for:
   - doctor parser duplicate `--online` and unknown flag failures,
   - mixed audit fixtures containing blank lines,
   - malformed JSON error context behavior.
2. Run targeted tests to confirm RED when assertions fail under current coverage.
3. Implement minimal test fixtures/assertions (no production logic change unless a defect is exposed).
4. Re-run targeted tests and full `tau-diagnostics` crate tests for GREEN/regression.

## Affected Paths
- `crates/tau-diagnostics/src/lib.rs`
- `specs/milestones/m180/index.md`
- `specs/3008/spec.md`
- `specs/3008/plan.md`
- `specs/3008/tasks.md`

## Risks and Mitigations
- Risk: brittle assertions tied to exact error strings.
  - Mitigation: assert stable high-signal substrings (line number + usage string).
- Risk: test fixture setup obscures behavior being asserted.
  - Mitigation: small inline fixtures and explicit counter assertions.

## Interfaces / Contracts
- Public parser contract: invalid doctor args fail closed with `DOCTOR_USAGE`.
- Audit summarizer contract: deterministic counters for recognized record types; malformed JSON errors include line context.

## ADR
Not required (test-only scope).
