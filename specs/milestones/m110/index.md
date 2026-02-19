# M110 - Tau Ops Dashboard PRD Foundation (Phase 1)

Status: In Progress
Related PRD: `specs/tau-ops-dashboard-prd.md`
Related roadmap item: `tasks/spacebot-comparison.md` (G18 Full Web Dashboard)

## Objective
Execute the first production implementation slice from the Tau Ops Dashboard PRD by adding PRD-aligned memory explorer API primitives to `tau-gateway` while preserving existing gateway dashboard/session compatibility.

## Issue Map
- Epic: #2665
- Story: #2666
- Task: #2667

## Deliverables
- Gateway memory entry CRUD endpoints:
  - `GET /gateway/memory/{session_key}/{entry_id}`
  - `PUT /gateway/memory/{session_key}/{entry_id}`
  - `DELETE /gateway/memory/{session_key}/{entry_id}`
- Queryable memory search mode on `GET /gateway/memory/{session_key}` with scope/type filters.
- Integration tests validating auth, policy gates, search filters, and backward compatibility for existing memory/session/dashboard endpoints.

## Exit Criteria
- #2665, #2666, and #2667 are closed.
- `specs/2667/spec.md` status is `Implemented`.
- Scoped verification gates pass with evidence (`fmt`, `clippy -p tau-gateway`, targeted tests).
- PRD checklist progress is updated for completed memory-explorer foundation items.
