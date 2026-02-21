# Spec: Issue #3078 - ops memory-graph node color type contracts

Status: Reviewed

## Problem Statement
`/ops/memory-graph` now exposes deterministic node/edge and node-size contracts,
but node-color contracts are not surfaced. PRD checklist contract `2089`
(node color reflects memory type) remains unverified.

## Scope
In scope:
- Add deterministic node-color marker attributes for memory graph nodes.
- Map memory types to stable color tokens and color values.
- Validate color-marker rendering via UI and gateway conformance tests.

Out of scope:
- Edge style and interactive graph behavior.
- New dependencies.

## Acceptance Criteria
### AC-1 `/ops/memory-graph` preserves deterministic graph contract surface
Given `/ops/memory-graph` renders,
when graph nodes are empty,
then deterministic panel/list markers remain stable.

### AC-2 Node color markers reflect memory type
Given memory graph nodes with different memory types,
when `/ops/memory-graph` is rendered,
then each node exposes deterministic color token/value markers mapped from memory type.

### AC-3 Non-memory-graph routes preserve hidden graph contracts
Given any non-memory-graph route renders,
when shell HTML is produced,
then graph panel markers remain present and hidden.

### AC-4 Existing memory graph and memory explorer contracts remain green
Given existing memory graph/explorer specs,
when node-color contracts are added,
then selected conformance/regression suites remain green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | `/ops/memory-graph` with no records | render route | graph markers remain deterministic with zero-count defaults |
| C-02 | AC-2 | Integration | graph nodes contain different memory types | render route | node rows include deterministic `data-node-color-token` and `data-node-color-hex` |
| C-03 | AC-3 | Regression | route is not `/ops/memory-graph` | render route | graph panel markers remain present and hidden |
| C-04 | AC-4 | Regression | existing memory specs | rerun selected suites | previous contracts remain green |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_3078 -- --test-threads=1`
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
