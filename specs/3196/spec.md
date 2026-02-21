# Spec: Issue #3196 - make panic/unsafe audit test-context aware

Status: Implemented

## Problem Statement
The panic/unsafe audit script misclassifies src-level test-only markers as non-test because it relies on path-only heuristics. This degrades trust in the metric and triggers noisy quality flags.

## Scope
In scope:
- Add fixture lines containing panic/unsafe markers inside `#[cfg(test)]` module in `src/*`.
- Update `scripts/dev/audit-panic-unsafe.sh` classification to treat `#[cfg(test)]`/`#[test]` contexts as test.
- Update fixture contract expectations in `scripts/dev/test-audit-panic-unsafe.sh`.

Out of scope:
- Any runtime Rust code changes outside audit scripts/fixtures.
- Policy threshold changes.

## Acceptance Criteria
### AC-1 src-level cfg(test) markers are classified as test context
Given a fixture file under `src/*` with panic/unsafe markers inside `#[cfg(test)]` module,
when running `scripts/dev/audit-panic-unsafe.sh`,
then those markers count toward `*_test_path` and not `*_non_test_path`.

### AC-2 existing fixture contract remains deterministic
Given the fixture conformance script,
when running `scripts/dev/test-audit-panic-unsafe.sh`,
then it passes with stable expected totals.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional/Conformance | fixture with src-level cfg(test) markers | run audit script | test-context counts increment; non-test counts unchanged for those markers |
| C-02 | AC-2 | Functional/Conformance | fixture conformance script | run fixture test script | script exits 0 with expected totals |

## Success Metrics / Observable Signals
- `scripts/dev/test-audit-panic-unsafe.sh`
- `scripts/dev/audit-panic-unsafe.sh`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
