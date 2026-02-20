# Spec: Issue #2726 - G19 phase-2 API parity for /api/memories/graph and force-layout rendering

Status: Implemented

## Problem Statement
Tau currently exposes memory graph data at `GET /gateway/memory-graph/{session_key}` and renders graph nodes in a circular layout in webchat. Spacebot parity checklist still expects `/api/memories/graph` route compatibility and force-layout visualization behavior for operational graph inspection.

## Acceptance Criteria

### AC-1 Gateway exposes `/api/memories/graph` compatibility endpoint
Given an authorized gateway request,
When `GET /api/memories/graph` is called with graph query filters,
Then the response returns deterministic graph `nodes` + `edges` JSON with filter metadata and auth/rate-limit enforcement.

### AC-2 Existing gateway graph route remains compatible
Given existing clients using `GET /gateway/memory-graph/{session_key}`,
When the parity route is introduced,
Then existing route behavior remains unchanged and regression tests stay green.

### AC-3 Webchat graph rendering uses force-layout positioning
Given graph payload data with nodes/edges,
When the memory graph view renders,
Then node positions are computed through iterative force-layout simulation (not static ring placement) while preserving deterministic sizing/color cues.

### AC-4 Node size and edge color cues remain explicit
Given graph payload records,
When graph renders,
Then node radius scales from node weight/importance signals and edge stroke color maps by relation type.

### AC-5 Scoped verification gates pass
Given this slice,
When scoped checks run,
Then `cargo fmt --check`, `cargo clippy -p tau-gateway -- -D warnings`, and targeted gateway tests for graph endpoint/rendering pass.

## Scope

### In Scope
- Add `/api/memories/graph` route handler in gateway.
- Add/extend graph endpoint tests for new route and compatibility.
- Refactor webchat SVG renderer to force-layout simulation.
- Keep existing graph payload contract stable for current UI consumers.
- Update `tasks/spacebot-comparison.md` G19 checklist lines completed by this slice.

### Out of Scope
- New frontend framework migration.
- New third-party visualization dependency packages.
- Cross-session/global graph federation beyond current session scope.

## Conformance Cases
- C-01 (integration): authorized `GET /api/memories/graph` returns expected graph payload fields.
- C-02 (regression): unauthorized `GET /api/memories/graph` is rejected.
- C-03 (regression): existing `/gateway/memory-graph/{session_key}` route still returns deterministic filtered graph.
- C-04 (unit): webchat page script includes force-layout simulation function and no static ring-only placement path.
- C-05 (functional): rendered node radius uses node size/importance signal and edges map stroke color by relation type.
- C-06 (verify): fmt/clippy/targeted tests pass.

## Success Metrics / Observable Signals
- Spacebot checklist G19 API path requirement is satisfied by parity route.
- Graph rendering better reflects topology using force-directed positions.
- No regressions in gateway auth and existing memory graph operations.
