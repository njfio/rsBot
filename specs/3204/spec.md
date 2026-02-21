# Spec: Issue #3204 - align panic policy audit classifier with per-line test context

Status: Implemented

## Problem Statement
The policy audit script uses a first-`#[cfg(test)]` line heuristic that can mark subsequent non-test lines as test context. This weakens panic/unsafe governance metrics used by guard policies.

## Scope
In scope:
- Extend fixture to include a non-test panic after a cfg(test) module.
- Update `panic-unsafe-audit.sh` classifier to parse line context and detect explicit test scopes.
- Keep output schema unchanged for guard consumers.

Out of scope:
- Policy threshold changes.
- Runtime code changes outside scripts/dev fixtures and audit/guard scripts.

## Acceptance Criteria
### AC-1 non-test markers after cfg(test) blocks remain review_required
Given a fixture file containing both cfg(test) module markers and later non-test panic,
when running `scripts/dev/panic-unsafe-audit.sh`,
then the later non-test panic is classified `review_required`.

### AC-2 guard pipeline compatibility remains intact
Given updated audit classifier,
when running `scripts/dev/test-panic-unsafe-guard.sh`,
then guard behavior and schema assumptions remain valid.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional/Conformance | fixture with mixed cfg(test) + non-test markers | run `test-panic-unsafe-audit.sh` | counts reflect non-test marker as review_required |
| C-02 | AC-2 | Functional/Conformance | guard fixture scripts | run `test-panic-unsafe-guard.sh` | guard tests pass unchanged |

## Success Metrics / Observable Signals
- `scripts/dev/test-panic-unsafe-audit.sh`
- `scripts/dev/test-panic-unsafe-guard.sh`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
