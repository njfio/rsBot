# Spec: Issue #2917 - ops memory create-entry contracts

Status: Reviewed

## Problem Statement
The PRD requires creating new memory entries with all fields from the ops dashboard, but `/ops/memory` currently exposes only search/filter contracts and no deterministic create-entry form or submission handling.

## Scope
In scope:
- Add deterministic create-entry form controls on `/ops/memory` for required memory fields.
- Implement gateway submission handling for the create form using full-field input mapping.
- Add deterministic success contracts (redirect or confirmation markers).

Out of scope:
- Edit/delete flows.
- Graph updates/visualization.
- New dependencies.

## Acceptance Criteria
### AC-1 Memory route exposes deterministic create-entry form contracts
Given `/ops/memory` renders,
when an operator opens the page,
then a deterministic create form exists with required fields and stable marker IDs.

### AC-2 Create submission persists memory entry with full-field mapping
Given valid form values for required fields,
when the create form is submitted,
then a new memory entry is persisted and appears via existing memory contracts.

### AC-3 Post-create behavior is deterministic and observable
Given a successful create,
when the response is rendered,
then deterministic success/route markers confirm the create path outcome.

### AC-4 Existing memory/search/scope/type contracts remain green
Given prior delivered memory contracts,
when create-entry contracts are added,
then existing regression suites remain green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | `/ops/memory` | render memory route | create form markers/fields are present with deterministic IDs |
| C-02 | AC-2 | Integration | valid create-form payload with all fields | submit create form | entry is persisted and discoverable via memory route contracts |
| C-03 | AC-3 | Functional | successful create submission | render redirected/returned memory view | success markers/state are deterministic |
| C-04 | AC-4 | Regression | existing memory/search/scope/type contracts | rerun selected suites | prior contracts remain green |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_2917 -- --test-threads=1` passes.
- `cargo test -p tau-gateway spec_2917 -- --test-threads=1` passes.
- Regression slice in `specs/2917/tasks.md` passes.
