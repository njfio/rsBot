# M110 - Tau Ops Dashboard PRD Foundation (Phase 1)

Status: In Progress
Related PRD: `specs/tau-ops-dashboard-prd.md`
Related roadmap item: `tasks/spacebot-comparison.md` (G18 Full Web Dashboard)

## Objective
Execute production implementation slices from the Tau Ops Dashboard PRD by adding PRD-aligned gateway API primitives while preserving existing gateway dashboard/session compatibility.

## Issue Map
- Epic: #2665
- Completed Story: #2666
- Completed Task: #2667
- Completed Story: #2669
- Completed Task: #2670
- Completed Story: #2672
- Completed Task: #2673
- Completed Story: #2675
- Completed Task: #2676

## Deliverables
- Completed (`#2667`):
  - Gateway memory entry CRUD endpoints:
    - `GET /gateway/memory/{session_key}/{entry_id}`
    - `PUT /gateway/memory/{session_key}/{entry_id}`
    - `DELETE /gateway/memory/{session_key}/{entry_id}`
  - Queryable memory search mode on `GET /gateway/memory/{session_key}` with scope/type filters.
- Completed (`#2670`):
  - Gateway channel lifecycle action endpoint:
    - `POST /gateway/channels/{channel}/lifecycle`
  - Status payload discovery field for lifecycle endpoint integration.
- Completed (`#2673`):
  - Gateway config endpoints:
    - `GET /gateway/config`
    - `PATCH /gateway/config`
  - Config patch apply semantics (`applied` vs `restart_required_fields`) and heartbeat hot-reload policy support.
- Completed (`#2676`):
  - Gateway safety policy endpoints:
    - `GET /gateway/safety/policy`
    - `PUT /gateway/safety/policy`
  - Policy persistence contract with validation and status discovery metadata.

## Exit Criteria
- Epic #2665 is closed with all scoped PRD phase-1 tasks completed.
- `specs/2667/spec.md`, `specs/2670/spec.md`, `specs/2673/spec.md`, and `specs/2676/spec.md` status are `Implemented`.
- Scoped verification gates pass with evidence (`fmt`, `clippy -p tau-gateway`, targeted tests).
- PRD checklist progress is updated for completed phase-1 endpoint slices.
