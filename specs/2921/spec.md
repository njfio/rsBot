# Spec: Issue #2921 - ops memory edit-entry contracts

Status: Reviewed

## Problem Statement
The PRD requires editing existing memory entries with all fields from the ops dashboard, but `/ops/memory` currently supports create + search/filter contracts only and has no deterministic edit-entry contracts.

## Scope
In scope:
- Add deterministic edit-entry form controls on `/ops/memory` for full-field updates.
- Implement gateway submission handling for edit form updates against existing entries.
- Add deterministic success contracts (redirect/confirmation markers).

Out of scope:
- Create/delete enhancements beyond existing contracts.
- Memory graph behavior changes.
- New dependencies.

## Acceptance Criteria
### AC-1 Memory route exposes deterministic edit-entry form contracts
Given `/ops/memory` renders,
when an operator opens the page,
then a deterministic edit form exists with stable marker IDs and full-field controls.

### AC-2 Edit submission updates memory entry with full-field mapping
Given an existing memory entry and valid edit form values,
when the edit form is submitted,
then the target memory entry is updated and observable via existing memory contracts.

### AC-3 Post-edit behavior is deterministic and observable
Given a successful edit,
when the response is rendered,
then deterministic success markers confirm edit outcome and edited target ID.

### AC-4 Existing memory/search/scope/type/create contracts remain green
Given prior delivered memory contracts,
when edit-entry contracts are added,
then existing regression suites remain green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | `/ops/memory` | render memory route | edit form markers/fields are present with deterministic IDs |
| C-02 | AC-2 | Integration | existing memory entry + valid edit payload | submit edit form | entry is updated with mapped fields and discoverable through route contracts |
| C-03 | AC-3 | Functional | successful edit submission | render redirected/returned memory view | success markers/state are deterministic |
| C-04 | AC-4 | Regression | existing memory/search/scope/type/create contracts | rerun selected suites | prior contracts remain green |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_2921 -- --test-threads=1` passes.
- `cargo test -p tau-gateway spec_2921 -- --test-threads=1` passes.
- Regression slice in `specs/2921/tasks.md` passes.
