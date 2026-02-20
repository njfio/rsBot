# M161 - Tau Ops Dashboard PRD Phase 3C (Memory Type Filter Contracts)

## Context
Implements Tau Ops Dashboard PRD Memory checklist item: "Type filter works" for `/ops/memory` contracts.

## Linked Issues
- Epic: #2911
- Story: #2912
- Task: #2913

## Scope
- Deterministic memory-type filter controls on `/ops/memory`.
- Preserved selected type-filter state in rendered markers.
- Integration conformance coverage proving type-filter narrowing behavior.
- Regression safety for prior memory search and scope-filter contracts.

## Out of Scope
- Memory CRUD workflows.
- Memory graph route behavior.
