# Plan: Issue #2838 - Sessions explorer deterministic row contracts

## Approach
1. Add RED conformance tests for `/ops/sessions` panel/list/row markers in `tau-dashboard-ui` and `tau-gateway`.
2. Add RED integration tests that seed multiple session files and validate deterministic row ordering and `/ops/chat` row links.
3. Extend UI shell render with dedicated `/ops/sessions` panel markers and explicit empty-state marker path.
4. Extend gateway shell snapshot collection to map discovered session keys for the sessions explorer route.
5. Run targeted regressions for existing `/ops/chat` phase 1N contracts and ops route suites.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_dashboard_shell.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks & Mitigations
- Risk: sessions row ordering becomes non-deterministic across filesystems.
  - Mitigation: sanitize, deduplicate, and sort keys before rendering.
- Risk: sessions explorer integration regresses chat selector contracts from phase 1N.
  - Mitigation: keep chat selectors intact and run `spec_2834` regression suites.
- Risk: empty-state contracts never trigger due implicit default session insertion.
  - Mitigation: add route-specific empty-state mapping based on actual discovered session files.

## Interfaces / Contracts
- New UI markers on `/ops/sessions`:
  - `id="tau-ops-sessions-panel"` with deterministic route/hidden markers.
  - `id="tau-ops-sessions-list"` with `data-session-count`.
  - `id="tau-ops-sessions-row-<index>"` rows with `data-session-key`.
  - `id="tau-ops-sessions-empty-state"` marker for empty-state path.
- Gateway shell mapping:
  - discover session keys from `state_dir/openresponses/sessions/*.jsonl`,
  - provide deterministic row data for `/ops/sessions` rendering.

## ADR
No ADR required: no dependency/protocol/architecture boundary changes.
