# M195 - Tau Ops Dashboard PRD Phase 3H (Memory Graph Nodes + Edges Contracts)

## Context
Implements Tau Ops Dashboard PRD memory-graph checklist contract:
- `2087` "Graph renders with nodes and edges"

for `/ops/memory-graph`.

## Linked Issues
- Epic: #3066
- Story: #3067
- Task: #3068

## Scope
- Deterministic memory-graph panel markers on `/ops/memory-graph`.
- Gateway-backed node and edge row hydration from session memory records.
- Deterministic node/edge row IDs and count markers for SSR validation.
- Regression safety for existing memory explorer contracts.

## Out of Scope
- Node-size semantics (`2088`) and color semantics (`2089`).
- Edge-style semantics (`2090`) and graph interactions (`2091`-`2095`).
- New memory runtime ranking or relation algorithms.
