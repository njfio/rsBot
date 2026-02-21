# Plan: Issue #3152 - Correct Review #35 unresolved claims and add property rate-limit invariants

## Approach
1. Add RED conformance script assertions for corrected unresolved entries in `tasks/review-35.md`.
2. Add RED property tests for rate-limit invariants in `tau-tools`.
3. Update `tasks/review-35.md` unresolved tracker rows to current state with evidence pointers.
4. Implement GREEN property-test logic and run scoped verification commands.

## Affected Modules
- `tasks/review-35.md`
- `scripts/dev/test-review-35.sh`
- `crates/tau-tools/src/tools/tests.rs`

## Risks & Mitigations
- Risk: over-constraining review doc checks to volatile wording.
  - Mitigation: assert deterministic row fragments and evidence anchors only.
- Risk: flaky property tests from wall-clock time coupling.
  - Mitigation: use `ToolPolicy::evaluate_rate_limit` with injected deterministic timestamps.

## Interfaces / Contracts
- Review #35 unresolved tracker row values (status + evidence) are part of the doc contract.
- `ToolPolicy::evaluate_rate_limit(principal, now_unix_ms)` invariant surface is validated via property tests.

## ADR
No ADR required.
