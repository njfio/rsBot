# M197 - Tau Ops Dashboard PRD Phase 3J (Memory Graph Node Color Contracts)

## Context
Implements Tau Ops Dashboard PRD memory-graph checklist contract:
- `2089` "Node color reflects memory type"

for `/ops/memory-graph`.

## Linked Issues
- Epic: #3076
- Story: #3077
- Task: #3078

## Scope
- Deterministic node-color markers on `/ops/memory-graph` node rows.
- Gateway memory-type to color-token mapping.
- Deterministic color token + hex marker values for SSR validation.
- Regression safety for existing memory-graph and memory-explorer contracts.

## Out of Scope
- Edge style semantics (`2090`).
- Graph interactions (`2091`-`2095`).
- New runtime ranking or relation algorithms.
