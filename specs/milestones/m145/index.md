# Milestone M145 - Tau Ops Dashboard PRD Phase 1Q (Session Graph Contracts)

Status: InProgress

## Scope
Implement Tau Ops `/ops/sessions/{session_key}` deterministic session graph contracts:
- dedicated session graph panel SSR markers for selected session key,
- deterministic graph node and edge row markers sourced from session lineage parent links,
- explicit graph empty-state marker behavior when selected session has no graph nodes.

## Linked Issues
- Epic: #2844
- Story: #2845
- Task: #2846

## Success Signals
- `/ops/sessions/{session_key}` HTML includes deterministic graph panel/list markers.
- Graph node rows and edge rows deterministically map from selected session lineage.
- Graph empty-state marker renders when selected session has no entries.
- Existing `/ops/sessions` detail/list and `/ops/chat` marker contracts remain green.
