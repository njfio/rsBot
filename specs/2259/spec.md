# Spec #2259

Status: Implemented
Milestone: specs/milestones/m46/index.md
Issue: https://github.com/njfio/Tau/issues/2259

## Problem Statement

`tau-session` exposes a `Postgres` backend selector, but read/write paths still fail with
"scaffolded but not implemented". This blocks production deployments that require
durable centralized session persistence across multiple hosts.

## Scope

In scope:

- Implement PostgreSQL-backed session entry persistence for `SessionStore`.
- Implement PostgreSQL-backed session usage summary persistence.
- Keep existing JSONL and SQLite behavior unchanged.
- Add conformance and integration coverage for backend selection, persistence, and
  isolation behavior.

Out of scope:

- Session schema migrations beyond initial table/bootstrap creation.
- Replacing local lock-file semantics with distributed lock orchestration.
- Changing CLI/session command contracts.

## Acceptance Criteria

- AC-1: Given `TAU_SESSION_BACKEND=postgres` and non-empty
  `TAU_SESSION_POSTGRES_DSN`, when `SessionStore::load` is called, then store backend is
  `SessionStorageBackend::Postgres` and no scaffold error is emitted.
- AC-2: Given a PostgreSQL-backed store, when messages are appended and store is
  reloaded, then lineage and entry ordering are preserved.
- AC-3: Given a PostgreSQL-backed store, when usage deltas are recorded and store is
  reloaded, then cumulative usage values are preserved.
- AC-4: Given two different session paths using the same PostgreSQL DSN, when each
  path writes entries, then reads are isolated per session path key.
- AC-5: Given invalid or unreachable PostgreSQL DSN while backend is `postgres`, when
  store operations run, then errors are explicit and actionable (no silent fallback to
  JSONL/SQLite).
- AC-6: Given JSONL/SQLite backends, when existing tests run, then behavior remains
  unchanged.

## Conformance Cases

- C-01 (AC-1, unit): `resolve_session_backend` selects `Postgres` when env config is
  set and DSN is non-empty.
- C-02 (AC-2, integration): PostgreSQL backend round-trip append/reload preserves
  lineage and IDs.
- C-03 (AC-3, integration): PostgreSQL usage summary round-trip preserves cumulative
  token/cost totals.
- C-04 (AC-4, integration): Two logical session paths in one DSN do not leak records.
- C-05 (AC-5, functional): Invalid DSN surfaces backend-specific failure text; backend
  does not downgrade automatically.
- C-06 (AC-6, regression): Existing SQLite and JSONL conformance tests remain green.

## Success Metrics / Observable Signals

- No `"session postgres backend is scaffolded but not implemented"` errors in
  `tau-session` code paths.
- `SessionStore::storage_backend()` returns `Postgres` when configured.
- Postgres conformance tests C-02..C-04 pass in environments with test DSN.
- Full `tau-session` crate test suite remains green for non-Postgres flows.
