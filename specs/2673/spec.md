# Spec: Issue #2673 - PRD gateway config GET/PATCH endpoints

Status: Implemented

## Problem Statement
`specs/tau-ops-dashboard-prd.md` defines `/gateway/config` (`GET` and `PATCH`) for operator configuration workflows. `tau-gateway` currently lacks this endpoint family, so dashboard operators cannot inspect active gateway config or submit structured config updates with explicit hot-reload vs restart-required semantics.

## Acceptance Criteria

### AC-1 Authenticated operators can read active gateway config
Given a valid authenticated request,
When `GET /gateway/config` is called,
Then the gateway returns an active config snapshot, hot-reload capability metadata, and any pending override state.

### AC-2 Patch requests accept valid config updates with explicit apply semantics
Given a valid authenticated patch request,
When `PATCH /gateway/config` is called with supported fields,
Then the gateway records pending overrides, applies hot-reloadable heartbeat policy updates immediately, and returns `applied` vs `restart_required_fields` deterministically.

### AC-3 Invalid patch payloads fail closed
Given malformed or unsupported patch payloads,
When `PATCH /gateway/config` is called,
Then the gateway returns `400` with explicit error codes and does not write partial config state.

### AC-4 Unauthorized requests are rejected
Given missing or invalid credentials,
When `GET` or `PATCH /gateway/config` is called,
Then the gateway returns `401` and does not expose or mutate config state.

### AC-5 Gateway status discovery includes config endpoint
Given an authenticated status request,
When `GET /gateway/status` is called,
Then the payload advertises the `/gateway/config` endpoint for dashboard discovery.

### AC-6 Scoped verification gates pass
Given this implementation slice,
When scoped checks run,
Then `cargo fmt --check`, `cargo clippy -p tau-gateway -- -D warnings`, and targeted gateway tests pass.

## Scope

### In Scope
- New gateway route template: `/gateway/config` (`GET`, `PATCH`).
- Config read response including active runtime snapshot and pending override metadata.
- Config patch handling for a bounded field set with deterministic validation and apply semantics.
- Heartbeat interval hot-reload policy file write path for immediate apply behavior.
- Status payload discovery update for config endpoint.
- Integration/regression tests for auth, validation, apply semantics, and status discovery.

### Out of Scope
- Full profile TOML editor semantics and complete runtime hot-reload across all config fields.
- Safety/training/audit/tools/jobs/deploy endpoint families.
- Leptos UI implementation.

## Conformance Cases
- C-01 (functional): `GET /gateway/config` returns active snapshot and capability metadata.
- C-02 (functional): `PATCH /gateway/config` with valid model + heartbeat interval returns deterministic `applied` and `restart_required_fields` response and writes policy/override state.
- C-03 (regression): `PATCH /gateway/config` with empty payload returns `400` `no_config_changes`.
- C-04 (regression): `PATCH /gateway/config` with invalid values (e.g., blank model or zero heartbeat interval) returns `400` and does not persist state.
- C-05 (regression): unauthorized `GET`/`PATCH` calls return `401`.
- C-06 (regression): `GET /gateway/status` includes `config_endpoint` while preserving existing web UI endpoint fields.
- C-07 (verify): scoped fmt/clippy/targeted tests pass.

## Success Metrics / Observable Signals
- Operators can inspect gateway config from the API without reading local files.
- Patch responses clearly separate hot-reload-applied updates from restart-required updates.
- Invalid payloads are rejected deterministically and do not cause partial writes.
