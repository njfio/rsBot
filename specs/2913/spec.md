# Spec: Issue #2913 - ops memory type-filter narrowing contracts

Status: Implemented

## Problem Statement
`/ops/memory` now supports query and scope filtering, but it lacks deterministic type-filter contracts and conformance coverage proving results can be narrowed by memory type as required by the PRD.

## Scope
In scope:
- Add deterministic memory-type filter form/query marker contracts.
- Preserve selected type filter value in rendered panel/form markers.
- Add integration coverage proving result narrowing by memory type.

Out of scope:
- Memory create/edit/delete workflows.
- Memory graph contracts.
- New dependencies.

## Acceptance Criteria
### AC-1 Memory route exposes deterministic type-filter control contracts
Given `/ops/memory` is rendered,
when an operator opens the memory search panel,
then deterministic type-filter controls are present and preserve selected values.

### AC-2 Type filter narrows persisted memory results
Given persisted memory entries across multiple memory types,
when `/ops/memory` renders with a selected type filter,
then result rows include only entries of that type.

### AC-3 No-match combinations preserve deterministic empty-state behavior
Given a selected type filter and query combination with no matches,
when `/ops/memory` renders,
then empty-state markers render with zero results.

### AC-4 Existing memory/scope and adjacent contracts remain green
Given previously delivered search and scope-filter contracts,
when type-filter contracts are added,
then regression suites remain green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | `/ops/memory?memory_type=<type>` | render memory route | deterministic type-filter controls/values are rendered |
| C-02 | AC-2 | Integration | persisted entries across memory types | render `/ops/memory` with selected `memory_type` | only matching type rows are rendered |
| C-03 | AC-3 | Functional | query + type combination with no matches | render memory route | empty-state marker is present and result count is zero |
| C-04 | AC-4 | Regression | existing contracts | rerun selected suites | prior contracts remain green |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_2913 -- --test-threads=1` passes.
- `cargo test -p tau-gateway spec_2913 -- --test-threads=1` passes.
- Regression slice in `specs/2913/tasks.md` passes.
