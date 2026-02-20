# Plan: Issue #2949 - Performance budget contract markers and conformance tests

## Approach
1. Add RED conformance tests for C-01..C-04 in `crates/tau-dashboard-ui/src/tests.rs`.
2. Add a dedicated performance contract marker section in `crates/tau-dashboard-ui/src/lib.rs`.
3. Ensure no existing route/panel IDs or behavior contracts regress.
4. Run scoped verify gates (fmt/clippy/tests).

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-dashboard-ui/src/tests.rs`
- `specs/milestones/m167/index.md`
- `specs/2949/spec.md`
- `specs/2949/tasks.md`

## Risks / Mitigations
- Risk: contract markers interpreted as runtime telemetry.
  - Mitigation: keep markers explicitly declarative and budget-focused.
- Risk: regression in existing dashboard contract suite.
  - Mitigation: run full `tau-dashboard-ui` tests in verify gate.

## Interfaces / Contracts
- No public API signature changes.
- Add SSR `data-*` contract markers for performance budgets.

## ADR
No ADR required: no dependency, protocol, or architecture boundary changes.
