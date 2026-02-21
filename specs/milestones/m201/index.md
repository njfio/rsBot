# M201 - Tau Ops Dashboard PRD Phase 3N (Memory Graph Zoom Contracts)

## Context
Implements Tau Ops Dashboard PRD memory-graph checklist contract:
- `2093` "Zoom in/out works"

for `/ops/memory-graph`.

## Linked Issues
- Epic: #3092
- Story: #3093
- Task: #3094

## Scope
- Deterministic graph zoom level and bounds markers.
- Deterministic zoom-in and zoom-out action links with clamped next-state values.
- Query-driven zoom level normalization for graph route rendering.
- UI/gateway conformance tests and regression coverage.

## Out of Scope
- Pan behavior (`2094`) and filter controls (`2095`).
- Canvas runtime interaction implementation.
- New dependencies.
