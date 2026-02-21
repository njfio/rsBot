# Plan: Issue #3112 - ops tools detail contracts

## Approach
1. Add RED UI tests for tools detail panel visibility, description/schema/policy markers, histogram, and invocation contracts.
2. Add RED gateway integration tests for `/ops/tools-jobs` detail contracts with fixture tool inventory data.
3. Extend dashboard snapshot with tool detail contract rows and defaults.
4. Render deterministic tool detail panel + sub-sections in UI shell.
5. Populate tool detail snapshot from gateway tool registry and deterministic fixture stats defaults.
6. Run regression suites and verify fmt/clippy gates.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-dashboard-ui/src/tests.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_dashboard_shell.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`
- `specs/milestones/m205/index.md`
- `specs/3112/spec.md`
- `specs/3112/plan.md`
- `specs/3112/tasks.md`

## Risks and Mitigations
- Risk: marker surface grows and causes brittle ordering behavior.
  - Mitigation: deterministic sorting of tools/histogram/invocation rows + explicit test assertions.
- Risk: route visibility regressions for existing dashboard sections.
  - Mitigation: assert hidden-state contracts on non-tools routes and rerun route regressions.
- Risk: ambiguous policy/stat values from runtime.
  - Mitigation: define deterministic fallback defaults for policy/stats in snapshot layer.

## Interface / Contract Notes
- Extend snapshot with tool detail fields:
  - selected tool id
  - description
  - parameter schema payload marker
  - policy timeout/max-output/sandbox markers
  - usage histogram rows
  - recent invocation rows
- Add tool detail markers:
  - `#tau-ops-tool-detail-panel`
  - `#tau-ops-tool-detail-metadata`
  - `#tau-ops-tool-detail-policy`
  - `#tau-ops-tool-detail-usage-histogram`
  - `#tau-ops-tool-detail-usage-bucket-<index>`
  - `#tau-ops-tool-detail-invocations`
  - `#tau-ops-tool-detail-invocation-row-<index>`
- P1 process rule: spec marked Reviewed; human review requested in PR.
