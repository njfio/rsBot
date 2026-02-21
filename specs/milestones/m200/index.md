# M200 - Tau Ops Dashboard PRD Phase 3M (Memory Graph Hover Highlight Contracts)

## Context
Implements Tau Ops Dashboard PRD memory-graph checklist contract:
- `2092` "Hover highlights connected edges"

for `/ops/memory-graph`.

## Linked Issues
- Epic: #3088
- Story: #3089
- Task: #3090

## Scope
- Deterministic edge highlight markers for active memory-graph focus context.
- Deterministic node neighbor markers for connected edges.
- UI/gateway conformance tests for highlight marker behavior.
- Regression safety for prior memory graph/explorer contracts.

## Out of Scope
- Zoom/pan/filter interactions (`2093`-`2095`).
- Runtime force-layout interaction implementation.
- New dependencies.
