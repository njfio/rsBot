# Spec: Issue #2667 - PRD memory explorer CRUD/search gateway endpoints

Status: Implemented

## Problem Statement
`specs/tau-ops-dashboard-prd.md` requires memory explorer workflows that can read, create/update, delete, and search typed memory entries. The current gateway memory API only supports a session-scoped markdown blob and does not expose entry-level CRUD/search semantics required by the dashboard PRD.

## Acceptance Criteria

### AC-1 Memory entries support entry-level CRUD by ID
Given a valid session key and memory entry ID,
When the operator calls new memory entry endpoints,
Then the gateway supports `GET`, `PUT`, and `DELETE` operations for that specific entry ID.

### AC-2 Memory search supports query + scope/type filtering
Given memory entries stored for a session,
When the operator calls `GET /gateway/memory/{session_key}` with a search query and optional filters,
Then the gateway returns a deterministic filtered search payload including typed entry metadata.

### AC-3 Auth and policy-gate enforcement remains fail-closed
Given dashboard auth and policy-gate requirements,
When new entry-write/delete operations are called without required auth or policy gate,
Then the gateway returns structured `401`/`403` errors and does not mutate state.

### AC-4 Backward compatibility for legacy memory surfaces is preserved
Given existing clients using blob memory and memory graph endpoints,
When the new entry-level endpoints are added,
Then existing `/gateway/memory/{session_key}` blob read/write and `/gateway/memory-graph/{session_key}` behavior remain functional.

### AC-5 Scoped verification gates pass
Given this implementation slice,
When scoped checks run,
Then `cargo fmt --check`, `cargo clippy -p tau-gateway -- -D warnings`, and targeted gateway tests pass.

## Scope

### In Scope
- New gateway memory entry endpoint template: `/gateway/memory/{session_key}/{entry_id}`.
- Entry-level `GET`, `PUT`, `DELETE` handlers backed by `tau-memory` runtime store semantics.
- Search mode support on `GET /gateway/memory/{session_key}` via query params.
- Integration/regression tests for auth, policy gates, CRUD/search behavior, and compatibility.

### Out of Scope
- Leptos UI crate creation and frontend route rendering.
- Safety/config/training/tools/channels/deploy endpoint families from the PRD.
- Multi-agent fleet management UI behavior.

## Conformance Cases
- C-01 (conformance): `PUT /gateway/memory/{session_key}/{entry_id}` with valid policy gate creates/updates an entry and returns typed metadata.
- C-02 (functional): `GET /gateway/memory/{session_key}/{entry_id}` returns stored entry metadata for existing IDs.
- C-03 (regression): `DELETE /gateway/memory/{session_key}/{entry_id}` requires policy gate and removes entry from readable active state.
- C-04 (functional): `GET /gateway/memory/{session_key}?query=...` returns filtered search results; `memory_type` filter narrows matches.
- C-05 (regression): unauthorized access to new endpoints returns `401` and does not mutate data.
- C-06 (regression): legacy blob memory read/write and memory graph endpoint behavior remain intact.
- C-07 (verify): scoped fmt/clippy/tests pass.

## Success Metrics / Observable Signals
- Operators can manage memory entries by ID via gateway API without direct file edits.
- Memory search payloads support PRD-aligned filter controls.
- Existing gateway memory/dashboard tests continue to pass.
