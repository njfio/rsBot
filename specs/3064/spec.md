# Spec: Issue #3064 - ops memory detail embedding and relations contracts

Status: Implemented

## Problem Statement
Tau Ops `/ops/memory` currently renders search/create/edit/delete contracts, but
does not expose a deterministic detail panel for selected memory entries.
PRD checklist contracts `2083` (embedding info) and `2084` (relations list)
remain unverified.

## Scope
In scope:
- Add deterministic detail-panel markers for selected memory entries.
- Resolve selected memory detail via gateway memory runtime read behavior.
- Surface embedding metadata markers (source/model/vector dimensions/reason).
- Surface relation-list row markers for connected entries.

Out of scope:
- Memory graph route behavior.
- New relevance/ranking algorithm changes.
- New external dependencies.

## Acceptance Criteria
### AC-1 `/ops/memory` exposes deterministic detail-panel contracts
Given the memory route renders,
when no memory is selected,
then a deterministic hidden/empty detail-panel contract is present.

### AC-2 Selected memory detail surfaces embedding metadata markers
Given a selected memory entry exists,
when the route is rendered with that entry selected,
then embedding metadata markers are visible and deterministic.

### AC-3 Selected memory detail surfaces relation-list row markers
Given a selected memory entry with relations,
when detail is rendered,
then relation-list rows are present with deterministic IDs and data attributes.

### AC-4 Existing memory contracts remain green
Given existing memory route contracts,
when detail-panel contracts are added,
then existing memory conformance/regression suites remain green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | `/ops/memory` without selection | render route | detail panel markers exist with hidden/empty defaults |
| C-02 | AC-2 | Integration | selected entry with embedding metadata | render route with selected id | embedding markers contain deterministic values |
| C-03 | AC-3 | Integration | selected entry with relations | render route with selected id | relation rows render with deterministic row markers |
| C-04 | AC-4 | Regression | existing memory specs | rerun selected suites | existing contracts remain green |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_3064 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3064 -- --test-threads=1`
- `cargo test -p tau-gateway spec_2905 -- --test-threads=1`
- `cargo test -p tau-gateway spec_2909 -- --test-threads=1`
- `cargo test -p tau-gateway spec_2913 -- --test-threads=1`
- `cargo test -p tau-gateway spec_2917 -- --test-threads=1`
- `cargo test -p tau-gateway spec_2921 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3060 -- --test-threads=1`
- `cargo fmt --check`
- `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`
