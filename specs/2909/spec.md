# Spec: Issue #2909 - ops memory scope-filter narrowing contracts

Status: Implemented

## Problem Statement
`/ops/memory` search contracts currently validate query and result rendering, but they do not expose deterministic scope-filter controls or explicit conformance proving workspace/channel/actor filters narrow matches as required by the PRD.

## Scope
In scope:
- Add deterministic scope-filter form/query marker contracts for `workspace_id`, `channel_id`, and `actor_id` on `/ops/memory`.
- Preserve selected scope-filter values in rendered form controls and panel attributes.
- Add integration coverage proving result narrowing by scope dimensions.

Out of scope:
- Memory type filtering.
- Memory entry create/edit/delete workflows.
- Graph visualization contracts.

## Acceptance Criteria
### AC-1 Memory route exposes deterministic scope-filter control contracts
Given `/ops/memory` is rendered,
when an operator opens the search panel,
then deterministic filter control markers for workspace/channel/actor are present and preserve requested values.

### AC-2 Scope filters narrow persisted memory search results
Given persisted memory entries across different workspace/channel/actor scopes,
when `/ops/memory` is rendered with scope-filter query parameters,
then result rows include only entries matching all specified filter dimensions.

### AC-3 Empty-state behavior remains deterministic under filter narrowing
Given a filter combination that yields no matches,
when `/ops/memory` renders,
then the empty-state marker is present and result count is zero.

### AC-4 Existing memory-search and adjacent route contracts remain green
Given prior ops/chat/sessions/search contracts,
when scope-filter contracts are added,
then existing regression suites remain green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | `/ops/memory?workspace_id=<w>&channel_id=<c>&actor_id=<a>` | render memory route | filter controls render with deterministic IDs/names/values |
| C-02 | AC-2 | Integration | persisted entries across mixed scopes | render `/ops/memory` with full filter tuple | only matching-scope rows are rendered |
| C-03 | AC-3 | Functional | filter tuple with no matches | render memory route | empty-state marker present and result count is zero |
| C-04 | AC-4 | Regression | existing ops dashboard contracts | rerun selected suites | existing contracts remain green |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_2909 -- --test-threads=1` passes.
- `cargo test -p tau-gateway spec_2909 -- --test-threads=1` passes.
- Regression slices from `specs/2909/tasks.md` pass.
