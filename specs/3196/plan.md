# Plan: Issue #3196 - make panic/unsafe audit test-context aware

## Approach
1. Add fixture markers in `src/*` under `#[cfg(test)]` module and update expected totals in fixture test (RED first).
2. Implement test-context detection in `scripts/dev/audit-panic-unsafe.sh` by parsing per-file line context for `#[cfg(test)]` and test attributes.
3. Re-run fixture script to GREEN and verify no regressions.
4. Run fmt and clippy checks.

## Affected Modules
- `scripts/dev/audit-panic-unsafe.sh`
- `scripts/dev/test-audit-panic-unsafe.sh`
- `scripts/dev/fixtures/panic-unsafe-audit/crates/sample/src/main.rs`
- `specs/milestones/m226/index.md`
- `specs/3196/spec.md`
- `specs/3196/plan.md`
- `specs/3196/tasks.md`

## Risks & Mitigations
- Risk: parser heuristic could misclassify complex macros/attributes.
  - Mitigation: keep algorithm conservative and additive; retain path-based classification as baseline and only promote to test-context when signal is explicit.
- Risk: output schema breakage.
  - Mitigation: preserve existing key names and report format.

## Interfaces / Contracts
- Script output contract keys:
  - `panic_total`, `panic_test_path`, `panic_non_test_path`
  - `unsafe_total`, `unsafe_test_path`, `unsafe_non_test_path`

## ADR
No ADR required (script-level classification fix only).
