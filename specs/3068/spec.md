# Spec: Issue #3068 - ops memory-graph nodes and edges contracts

Status: Implemented

## Problem Statement
Tau Ops includes a `/ops/memory-graph` route and navigation marker, but the
route does not expose deterministic SSR graph panel contracts with gateway-backed
node and edge rows. PRD checklist contract `2087` remains unverified.

## Scope
In scope:
- Add deterministic `/ops/memory-graph` panel markers.
- Hydrate memory graph node and edge rows from gateway memory records.
- Expose deterministic node/edge row IDs and count attributes.

Out of scope:
- Node-size and color semantics.
- Graph interactions (click/hover/zoom/pan/filter controls).
- New dependencies.

## Acceptance Criteria
### AC-1 `/ops/memory-graph` exposes deterministic graph-panel contracts
Given the memory-graph route renders,
when no graph records are available,
then deterministic panel, node-list, edge-list, and empty-state markers are present.

### AC-2 Memory graph node and edge rows render from gateway memory records
Given session memory records with relations exist,
when `/ops/memory-graph` is rendered,
then deterministic node and edge row markers reflect those records.

### AC-3 Non-memory-graph routes preserve hidden graph contracts
Given any other ops route renders,
when the shell is SSR-rendered,
then memory-graph panel markers remain present and hidden.

### AC-4 Existing memory explorer contracts remain green
Given prior memory route specs,
when memory-graph contracts are added,
then selected memory conformance/regression suites remain green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | `/ops/memory-graph` without records | render route | graph panel + list markers exist with zero-count defaults |
| C-02 | AC-2 | Integration | session contains related memory records | render `/ops/memory-graph` | deterministic node rows + edge rows render with expected IDs/data attrs |
| C-03 | AC-3 | Regression | route is not `/ops/memory-graph` | render route | graph panel markers remain present with hidden state |
| C-04 | AC-4 | Regression | existing memory specs | rerun selected suites | existing contracts remain green |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_3068 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3068 -- --test-threads=1`
- `cargo test -p tau-gateway spec_2905 -- --test-threads=1`
- `cargo test -p tau-gateway spec_2909 -- --test-threads=1`
- `cargo test -p tau-gateway spec_2913 -- --test-threads=1`
- `cargo test -p tau-gateway spec_2917 -- --test-threads=1`
- `cargo test -p tau-gateway spec_2921 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3060 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3064 -- --test-threads=1`
- `cargo fmt --check`
- `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`
