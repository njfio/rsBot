# M160 - Tau Ops Dashboard PRD Phase 3B (Memory Scope Filter Contracts)

## Context
Implements Tau Ops Dashboard PRD Memory checklist item: "Scope filters narrow results correctly" for `/ops/memory` contracts.

## Linked Issues
- Epic: #2907
- Story: #2908
- Task: #2909

## Scope
- Deterministic workspace/channel/actor filter controls on `/ops/memory`.
- Deterministic marker/state contracts preserving selected filter values.
- Integration coverage proving persisted memory results narrow by filter dimensions.
- Regression safety for previously delivered ops memory search contracts.

## Out of Scope
- Memory type filters.
- Memory CRUD flows.
- Memory graph rendering.
