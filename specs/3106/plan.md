# Plan: Issue #3106 - ops tools inventory contracts

## Approach
1. Add RED UI tests for `/ops/tools-jobs` panel visibility and deterministic inventory markers.
2. Add RED gateway integration tests validating inventory rows reflect registered tools.
3. Extend dashboard chat snapshot with tools inventory rows and summary metrics.
4. Render deterministic tools inventory panel/table in UI shell.
5. Populate inventory rows from gateway tool registrar and keep deterministic ordering.
6. Run regression suites and verify fmt/clippy gates.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-dashboard-ui/src/tests.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_dashboard_shell.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`
- `specs/milestones/m204/index.md`
- `specs/3106/spec.md`
- `specs/3106/plan.md`
- `specs/3106/tasks.md`

## Risks and Mitigations
- Risk: tools list order instability causing flaky tests.
  - Mitigation: sort tool names deterministically before rendering and assert by marker order/count.
- Risk: route panel visibility regressions for existing routes.
  - Mitigation: add explicit non-tools hidden panel assertions and rerun route regression suites.
- Risk: snapshot expansion breaking defaults.
  - Mitigation: add default values in snapshot `Default` impl and verify compile/test coverage.

## Interface / Contract Notes
- Add `TauOpsDashboardToolInventoryRow` to snapshot surface with deterministic row fields.
- Add snapshot fields for tool panel summary (`total_tools`) and inventory rows.
- Add tools panel markers:
  - `#tau-ops-tools-panel`
  - `#tau-ops-tools-inventory-summary`
  - `#tau-ops-tools-inventory-table`
  - `#tau-ops-tools-inventory-row-<index>`
  - `#tau-ops-tools-inventory-empty-state`
- P1 process rule: spec marked Reviewed; human review requested in PR.
