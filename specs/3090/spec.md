# Spec: Issue #3090 - ops memory-graph hover highlight contracts

Status: Reviewed

## Problem Statement
`/ops/memory-graph` now exposes deterministic node/edge contracts, size/color/style,
and node detail panel contracts, but does not expose deterministic contracts indicating
which edges/nodes are highlighted for the active graph focus context.
PRD checklist contract `2092` remains unverified.

## Scope
In scope:
- Add deterministic edge highlight markers for focused memory graph context.
- Add deterministic node neighbor markers for focused memory graph context.
- Validate hover-highlight contract rendering via UI and gateway tests.

Out of scope:
- Runtime pointer events or browser-only hover logic.
- New dependencies.

## Acceptance Criteria
### AC-1 `/ops/memory-graph` preserves deterministic graph contract surface
Given `/ops/memory-graph` renders with no active detail focus,
when graph nodes/edges are rendered,
then highlight markers remain present with default non-highlighted values.

### AC-2 Focused memory context highlights connected edges and neighbor nodes
Given `/ops/memory-graph` renders with an active detail memory ID,
when graph nodes/edges are rendered,
then edges connected to the focused memory are marked highlighted and connected nodes are marked neighbor-highlighted.

### AC-3 Non-memory-graph routes preserve hidden graph contracts
Given any non-memory-graph route renders,
when shell HTML is produced,
then graph panel markers remain present and hidden.

### AC-4 Existing memory graph and memory explorer contracts remain green
Given existing memory graph/explorer specs,
when hover-highlight contracts are added,
then selected conformance/regression suites remain green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | `/ops/memory-graph` without active detail focus | render route | edge/node highlight markers remain deterministic with `false` defaults |
| C-02 | AC-2 | Integration | graph route includes active detail memory ID | render route | connected edges expose `data-edge-hover-highlighted="true"` and connected nodes expose `data-node-hover-neighbor="true"` |
| C-03 | AC-3 | Regression | route is not `/ops/memory-graph` | render route | graph panel markers remain present and hidden |
| C-04 | AC-4 | Regression | existing memory specs | rerun selected suites | prior contracts remain green |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_3090 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3090 -- --test-threads=1`
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
