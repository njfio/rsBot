# Spec: Issue #3112 - ops tools detail contracts

Status: Reviewed

## Problem Statement
`/ops/tools-jobs` now lists available tools, but it does not expose deterministic
tool-detail contracts for description/schema/policy, usage histogram, and recent
invocations. PRD checklist contract `2099` remains unverified.

## Scope
In scope:
- Add deterministic tool detail panel markers for selected tool on `/ops/tools-jobs`.
- Add deterministic detail contracts for description, schema, and policy configuration.
- Add deterministic usage histogram markers (24h buckets contract surface).
- Add deterministic recent invocation row contracts (timestamp/args/result/duration/status).
- Populate detail snapshot data from gateway runtime inputs.
- Validate contracts via UI and gateway conformance tests.

Out of scope:
- Jobs tab contracts (`2100`-`2102`).
- Persisted analytics store changes.
- New dependencies.

## Acceptance Criteria
### AC-1 `/ops/tools-jobs` renders deterministic tool detail panel contracts
Given `/ops/tools-jobs` renders,
when shell HTML is produced,
then tool detail markers expose selected tool id and panel visibility contracts.

### AC-2 tool detail contracts expose description/schema/policy markers
Given selected tool detail data is present,
when shell HTML is produced,
then description, parameter schema, and policy config markers render deterministic values.

### AC-3 tool detail usage contracts expose histogram and recent invocations
Given selected tool usage data is present,
when shell HTML is produced,
then histogram bucket markers and recent invocation row markers render deterministic values.

### AC-4 non-tools routes preserve hidden tool detail contracts and prior suites remain green
Given any non-`/ops/tools-jobs` route renders,
when shell HTML is produced,
then tool detail markers remain present and hidden, and selected regression suites stay green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | active route is `/ops/tools-jobs` | render shell | tool detail panel markers are visible with deterministic selected-tool contract |
| C-02 | AC-2 | Functional | selected tool detail data exists | render shell | description/schema/policy markers render deterministic values |
| C-03 | AC-3 | Integration | gateway provides usage histogram + recent invocation rows | render `/ops/tools-jobs` | histogram and invocation markers render deterministic values |
| C-04 | AC-4 | Regression | active route is not `/ops/tools-jobs` | render shell | detail markers stay present/hidden; regression suites remain green |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_3112 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3112 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3106 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3106 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3103 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3103 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui functional_spec_2794_c02_c03_route_context_tokens_match_expected_values -- --test-threads=1`
- `cargo test -p tau-gateway functional_spec_2794_c01_c02_c03_all_sidebar_ops_routes_return_shell_with_route_markers -- --test-threads=1`
- `cargo fmt --check`
- `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`
