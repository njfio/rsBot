# Spec: Issue #2905 - ops memory search relevant result contracts

Status: Implemented

## Problem Statement
The PRD requires Memory Explorer search to return relevant results, but the Tau Ops shell currently lacks explicit memory search panel/result contracts and corresponding route-level conformance coverage.

## Scope
In scope:
- Add deterministic `/ops/memory` search panel contracts (form/query/result markers).
- Add gateway integration tests proving persisted memory entries surface as relevant search rows.
- Add empty-state contracts for no-match searches.

Out of scope:
- Memory graph route behavior.
- Memory entry create/edit/delete UI workflows.
- New dependencies.

## Acceptance Criteria
### AC-1 Memory route exposes deterministic search form contracts
Given `/ops/memory` is rendered,
when an operator loads the route with a search query,
then the memory panel exposes deterministic form/query markers preserving the requested query.

### AC-2 Relevant persisted memory matches render as deterministic result rows
Given persisted memory entries and a matching query,
when `/ops/memory` renders,
then relevant matches appear as deterministic result rows with stable metadata contracts.

### AC-3 No-match searches expose empty-state contracts
Given a search query with no matches,
when `/ops/memory` renders,
then a deterministic empty-state marker is shown and result row count is zero.

### AC-4 Existing route contracts remain intact
Given existing ops/chat/sessions/detail contracts,
when memory search contracts are added,
then existing suites remain green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | `/ops/memory?query=<q>` | render memory route | form/action/query markers are present and query preserved |
| C-02 | AC-2 | Integration | persisted memory entries containing query terms | render `/ops/memory?query=<q>` | deterministic result rows include relevant entries |
| C-03 | AC-3 | Functional | query with no matches | render memory route | empty-state marker present and result count zero |
| C-04 | AC-4 | Regression | existing contracts | rerun selected suites | chat/session/dashboard contracts remain green |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_2905 -- --test-threads=1` passes.
- `cargo test -p tau-gateway spec_2905 -- --test-threads=1` passes.
- Required regression slices remain green.
