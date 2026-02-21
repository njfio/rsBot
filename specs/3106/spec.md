# Spec: Issue #3106 - ops tools inventory contracts

Status: Reviewed

## Problem Statement
`/ops/tools-jobs` route is wired in navigation and breadcrumbs, but it does not
render a dedicated tools inventory panel that lists registered tools with
deterministic contracts. PRD checklist contract `2098` remains unverified.

## Scope
In scope:
- Add deterministic `/ops/tools-jobs` panel markers for tool inventory.
- Add deterministic tools inventory table and row contract markers.
- Populate dashboard snapshot with all registered tool names from gateway runtime.
- Validate contracts through UI and gateway conformance tests.

Out of scope:
- Tool detail policy/usage drill-down contracts (`2099`).
- Jobs tab contracts (`2100`-`2102`).
- New dependencies.

## Acceptance Criteria
### AC-1 `/ops/tools-jobs` exposes deterministic inventory panel contracts
Given `/ops/tools-jobs` renders,
when shell HTML is produced,
then a tools panel marker is visible with deterministic inventory summary markers.

### AC-2 tools inventory rows list all registered tools
Given gateway runtime has registered tool names,
when `/ops/tools-jobs` shell HTML is produced,
then table row markers enumerate all registered tools in deterministic order.

### AC-3 non-tools routes preserve hidden tools panel contracts
Given any non-`/ops/tools-jobs` route renders,
when shell HTML is produced,
then tools panel markers remain present and hidden.

### AC-4 existing ops shell route contracts remain green
Given existing route/spec contract suites,
when tools inventory contracts are added,
then selected regression suites remain green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | active route is `/ops/tools-jobs` | render shell | tools panel and summary markers are visible |
| C-02 | AC-2 | Integration | gateway has registered tools | render `/ops/tools-jobs` | tool rows list all registered tools deterministically |
| C-03 | AC-3 | Regression | active route is not `/ops/tools-jobs` | render shell | tools panel markers remain present and hidden |
| C-04 | AC-4 | Regression | existing route contracts | rerun selected suites | prior contracts remain green |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_3106 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3106 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3103 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3103 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3099 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3099 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui functional_spec_2794_c02_c03_route_context_tokens_match_expected_values -- --test-threads=1`
- `cargo test -p tau-gateway functional_spec_2794_c01_c02_c03_all_sidebar_ops_routes_return_shell_with_route_markers -- --test-threads=1`
- `cargo fmt --check`
- `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`
