# M115 - Spacebot G19 API Parity (/api/memories/graph)

Status: Active

## Context
G19 parity previously landed under `#2617` with route `GET /gateway/memory-graph/{session_key}` and SVG graph rendering. `tasks/spacebot-comparison.md` still tracks unresolved G19 checklist items because Spacebot parity expects `/api/memories/graph` compatibility and explicit force-layout visualization cues.

## Source
- `tasks/spacebot-comparison.md` (G19 Memory Graph Visualization)

## Objective
Close remaining G19 parity items by adding `GET /api/memories/graph` support in gateway and updating graph rendering to deterministic force-layout behavior while preserving auth boundaries.

## Scope
- Add `/api/memories/graph` endpoint compatibility path in gateway.
- Reuse/align existing graph payload schema and filter behavior.
- Update webchat graph rendering to force-layout behavior with relation-type edge colors and importance-driven node sizing.
- Extend tests and verify no regressions in existing gateway graph route/auth behavior.

## Issue Map
- Epic: #2724
- Story: #2723
- Task: #2726

## Acceptance Signals
- `/api/memories/graph` returns deterministic node/edge JSON for authorized requests.
- Webchat memory graph renders force-directed positions with stable relation/size cues.
- Existing `/gateway/memory-graph/{session_key}` behavior remains backward compatible.
