# Spec: Issue #3103 - ops memory-graph filter contracts

Status: Implemented

## Problem Statement
`/ops/memory-graph` currently supports deterministic structure, focus/highlight,
zoom, and pan contracts, but lacks deterministic filter controls that update graph
state contracts for memory type and relation type. PRD checklist contract `2095`
remains unverified.

## Scope
In scope:
- Add deterministic filter state markers for memory graph route.
- Add deterministic filter action links for memory-type and relation-type controls.
- Parse and normalize filter query state for graph route shell rendering.
- Apply filters to graph node/edge contract views.
- Validate filter contracts via UI and gateway conformance tests.

Out of scope:
- Runtime JS graph filtering behavior.
- New graph relation semantics.

## Acceptance Criteria
### AC-1 `/ops/memory-graph` exposes deterministic default filter contracts
Given `/ops/memory-graph` renders without explicit filter query,
when shell HTML is produced,
then filter markers render defaults with `memory_type=all` and `relation_type=all`.

### AC-2 filter query state updates graph contracts and action links
Given `/ops/memory-graph` renders with explicit filter query,
when shell HTML is produced,
then filter state is normalized and graph node/edge contract outputs reflect selected filters,
and filter action links preserve route state while updating target filter values.

### AC-3 Non-memory-graph routes preserve hidden graph contracts
Given any non-memory-graph route renders,
when shell HTML is produced,
then graph panel markers remain present and hidden.

### AC-4 Existing memory graph/explorer contracts remain green
Given existing memory graph/explorer specs,
when filter contracts are added,
then selected conformance/regression suites remain green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | graph route without filter query | render route | filter markers expose default contracts |
| C-02 | AC-2 | Integration | graph route with `graph_filter_memory_type` and `graph_filter_relation_type` query | render route | state and filter action links reflect normalized filter values; graph contracts reflect filters |
| C-03 | AC-3 | Regression | route is not `/ops/memory-graph` | render route | graph panel markers remain present and hidden |
| C-04 | AC-4 | Regression | existing memory specs | rerun selected suites | prior contracts remain green |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_3103 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3103 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3099 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3099 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3094 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3094 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3090 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3090 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3086 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3086 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3082 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3082 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3078 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3078 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3070 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3070 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3068 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3068 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3064 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3064 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3060 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3060 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_2921 -- --test-threads=1`
- `cargo test -p tau-gateway spec_2921 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_2917 -- --test-threads=1`
- `cargo test -p tau-gateway spec_2917 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_2913 -- --test-threads=1`
- `cargo test -p tau-gateway spec_2913 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_2909 -- --test-threads=1`
- `cargo test -p tau-gateway spec_2909 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_2905 -- --test-threads=1`
- `cargo test -p tau-gateway spec_2905 -- --test-threads=1`
- `cargo fmt --check`
- `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`
