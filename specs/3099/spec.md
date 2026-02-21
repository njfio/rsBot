# Spec: Issue #3099 - ops memory-graph pan contracts

Status: Reviewed

## Problem Statement
`/ops/memory-graph` currently supports deterministic graph structure, focus/highlight,
and zoom contracts, but lacks deterministic pan state and pan action contracts.
PRD checklist contract `2094` remains unverified.

## Scope
In scope:
- Add deterministic pan state markers for memory graph route.
- Add deterministic pan directional links with clamped next-state values.
- Parse and normalize pan query state for graph route shell rendering.
- Validate pan contracts via UI and gateway conformance tests.

Out of scope:
- Zoom and filter behavior contracts.
- Runtime JS/canvas drag physics.

## Acceptance Criteria
### AC-1 `/ops/memory-graph` exposes deterministic default pan contracts
Given `/ops/memory-graph` renders without explicit pan query,
when shell HTML is produced,
then pan markers render defaults with `x=0.00`, `y=0.00`, `step=25.00`.

### AC-2 pan query state drives clamped directional action contracts
Given `/ops/memory-graph` renders with explicit pan query,
when shell HTML is produced,
then pan state is clamped to bounds and left/right/up/down links encode clamped next values.

### AC-3 Non-memory-graph routes preserve hidden graph contracts
Given any non-memory-graph route renders,
when shell HTML is produced,
then graph panel markers remain present and hidden.

### AC-4 Existing memory graph and memory explorer contracts remain green
Given existing memory graph/explorer specs,
when pan contracts are added,
then selected conformance/regression suites remain green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | graph route without pan query | render route | pan markers expose default contracts |
| C-02 | AC-2 | Integration | graph route with `graph_pan_x`/`graph_pan_y` query | render route | state and directional pan links reflect clamped next states |
| C-03 | AC-3 | Regression | route is not `/ops/memory-graph` | render route | graph panel markers remain present and hidden |
| C-04 | AC-4 | Regression | existing memory specs | rerun selected suites | prior contracts remain green |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_3099 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3099 -- --test-threads=1`
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
