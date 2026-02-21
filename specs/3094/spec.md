# Spec: Issue #3094 - ops memory-graph zoom in/out contracts

Status: Reviewed

## Problem Statement
`/ops/memory-graph` currently exposes deterministic graph structure and focus/highlight
contracts, but lacks deterministic zoom state and zoom action contracts.
PRD checklist contract `2093` remains unverified.

## Scope
In scope:
- Add deterministic zoom level/bounds markers for memory graph route.
- Add deterministic zoom in/out links with clamped next-level values.
- Parse and normalize zoom query state for graph route shell rendering.
- Validate zoom contracts via UI and gateway conformance tests.

Out of scope:
- Pan and filter behavior contracts.
- JS runtime zoom physics.

## Acceptance Criteria
### AC-1 `/ops/memory-graph` exposes deterministic default zoom contracts
Given `/ops/memory-graph` renders without explicit zoom query,
when shell HTML is produced,
then zoom markers render defaults with level `1.00`, min `0.25`, max `2.00`, step `0.10`.

### AC-2 zoom query state drives clamped in/out action contracts
Given `/ops/memory-graph` renders with explicit zoom query,
when shell HTML is produced,
then zoom level is clamped to bounds and zoom-in/out links encode clamped next values.

### AC-3 Non-memory-graph routes preserve hidden graph contracts
Given any non-memory-graph route renders,
when shell HTML is produced,
then graph panel markers remain present and hidden.

### AC-4 Existing memory graph and memory explorer contracts remain green
Given existing memory graph/explorer specs,
when zoom contracts are added,
then selected conformance/regression suites remain green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | graph route without zoom query | render route | zoom markers expose default contracts |
| C-02 | AC-2 | Integration | graph route with `graph_zoom` query | render route | level and zoom action links reflect clamped next states |
| C-03 | AC-3 | Regression | route is not `/ops/memory-graph` | render route | graph panel markers remain present and hidden |
| C-04 | AC-4 | Regression | existing memory specs | rerun selected suites | prior contracts remain green |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_3094 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3094 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3090 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3086 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3082 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3078 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3070 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3068 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3064 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3060 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_2921 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_2917 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_2913 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_2909 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_2905 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3090 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3086 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3082 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3078 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3070 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3068 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3064 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3060 -- --test-threads=1`
- `cargo test -p tau-gateway spec_2921 -- --test-threads=1`
- `cargo test -p tau-gateway spec_2917 -- --test-threads=1`
- `cargo test -p tau-gateway spec_2913 -- --test-threads=1`
- `cargo test -p tau-gateway spec_2909 -- --test-threads=1`
- `cargo test -p tau-gateway spec_2905 -- --test-threads=1`
- `cargo fmt --check`
- `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`
