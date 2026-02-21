# M194 - Tau Ops Dashboard PRD Phase 3G (Memory Detail Embedding + Relations Contracts)

## Context
Implements Tau Ops Dashboard PRD memory checklist contracts:
- `2083` "Memory detail shows embedding info"
- `2084` "Relations list shows connected entries"

for `/ops/memory`.

## Linked Issues
- Epic: #3062
- Story: #3063
- Task: #3064

## Scope
- Deterministic memory detail panel markers on `/ops/memory`.
- Gateway-backed selection flow for a specific memory entry detail view.
- Embedding metadata markers and relation row markers for selected entries.
- Regression safety for existing memory search/filter/create/edit/delete contracts.

## Out of Scope
- Memory graph visualization contracts.
- New memory storage formats or migration behavior.
- New external dependencies.
