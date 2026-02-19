# Plan: Issue #2667 - PRD memory explorer CRUD/search gateway endpoints

## Approach
1. Add RED integration tests for entry-level CRUD/search behavior and auth/policy-gate enforcement.
2. Extend `tau-gateway` route table with entry-level memory endpoint template.
3. Implement typed memory entry CRUD handlers backed by `tau-memory::runtime::FileMemoryStore`.
4. Add search mode to `GET /gateway/memory/{session_key}` while preserving existing blob behavior when no query is provided.
5. Re-run scoped checks and confirm no regressions on existing session/memory/dashboard integration suites.

## Affected Modules
- `crates/tau-gateway/Cargo.toml`
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/types.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`
- `specs/milestones/m110/index.md`
- `specs/2667/spec.md`
- `specs/2667/plan.md`
- `specs/2667/tasks.md`

## Risks / Mitigations
- Risk: path-template collisions between `/gateway/memory/{session_key}` and `/gateway/memory/{session_key}/{entry_id}`.
  - Mitigation: explicit route template ordering and integration tests for both paths.
- Risk: introducing typed store semantics could break legacy markdown memory behavior.
  - Mitigation: keep legacy handlers operational; search mode only activates when query text is present.
- Risk: entry delete semantics could accidentally hard-delete history.
  - Mitigation: use soft-delete behavior via `tau-memory` runtime API.

## Interfaces / Contracts
- New endpoint contract:
  - `GET /gateway/memory/{session_key}/{entry_id}`
  - `PUT /gateway/memory/{session_key}/{entry_id}`
  - `DELETE /gateway/memory/{session_key}/{entry_id}`
- Extended contract:
  - `GET /gateway/memory/{session_key}` supports search mode when query text is supplied.
- Existing compatibility contract retained:
  - `GET/PUT /gateway/memory/{session_key}` blob behavior without search query.
  - `GET /gateway/memory-graph/{session_key}` unchanged.

## ADR
- Not required for this slice (internal endpoint extension, no external dependency version upgrades).
