# Spec: Issue #2923 - split tau-dashboard-ui lib below oversized-file threshold

Status: Implemented

## Problem Statement
`crates/tau-dashboard-ui/src/lib.rs` exceeded the 4000-line CI threshold, causing the oversized-file guard to fail and blocking continued PRD delivery velocity.

## Scope
In scope:
- Refactor `tau-dashboard-ui` test organization so `crates/tau-dashboard-ui/src/lib.rs` is <= 4000 lines.
- Preserve existing dashboard behavior and existing spec-linked test contracts.
- Remove temporary oversized-file exemption for `crates/tau-dashboard-ui/src/lib.rs`.

Out of scope:
- Behavioral feature changes for dashboard routes.
- New dependencies.
- Protocol/schema changes.

## Acceptance Criteria
### AC-1 `lib.rs` line count is within policy threshold
Given the production threshold is 4000 lines,
when CI checks `crates/tau-dashboard-ui/src/lib.rs`,
then the file line count is <= 4000 without exemption.

### AC-2 Existing behavior contracts remain unchanged
Given existing ops dashboard contracts,
when relevant spec suites are rerun,
then `spec_2921` and selected memory/session regression slices remain green.

### AC-3 Oversized-file policy check passes without ui exemption
Given oversized-file guard policy inputs,
when `.github/scripts/oversized_file_guard.py` is run,
then there are zero issues and no exemption entry for `crates/tau-dashboard-ui/src/lib.rs`.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | current `tau-dashboard-ui` source | line-count measured | `crates/tau-dashboard-ui/src/lib.rs` <= 4000 lines |
| C-02 | AC-2 | Regression | existing dashboard contracts | rerun `spec_2921` + selected regression slices | tests pass unchanged |
| C-03 | AC-3 | Functional | oversized-file policy script inputs | run policy guard | zero issues; no ui exemption metadata needed |

## Success Metrics / Signals
- `wc -l crates/tau-dashboard-ui/src/lib.rs` reports <= 4000.
- `cargo test -p tau-dashboard-ui spec_2921 -- --test-threads=1` passes.
- `cargo test -p tau-gateway spec_2921 -- --test-threads=1` passes.
- `python3 .github/scripts/oversized_file_guard.py ...` reports `issues=0`.
