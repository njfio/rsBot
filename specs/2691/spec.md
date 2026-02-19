# Spec: Issue #2691 - PRD gateway tools inventory and stats endpoints

Status: Implemented

## Problem Statement
`specs/tau-ops-dashboard-prd.md` API contract requires tool observability surfaces:
- `GET /gateway/tools`
- `GET /gateway/tools/stats`

`tau-gateway` currently lacks these endpoints, limiting dashboard-side tool inventory and usage diagnostics.

## Acceptance Criteria

### AC-1 Authenticated operators can fetch tool inventory
Given a valid authenticated request,
When `GET /gateway/tools` is called,
Then the gateway returns deterministic inventory derived from the configured tool registrar.

### AC-2 Authenticated operators can fetch tool usage stats
Given a valid authenticated request,
When `GET /gateway/tools/stats` is called,
Then the gateway returns deterministic per-tool aggregate usage stats payload.

### AC-3 Tool stats endpoint fails open with deterministic fallback diagnostics
Given missing or malformed telemetry artifacts,
When `GET /gateway/tools/stats` is called,
Then the endpoint returns `200` with deterministic diagnostics and stable aggregate shape.

### AC-4 Unauthorized tool inventory/stats requests are rejected
Given missing or invalid auth,
When `GET /gateway/tools` or `GET /gateway/tools/stats` are called,
Then the gateway returns fail-closed `401`.

### AC-5 Gateway status discovery advertises tools endpoints
Given an authenticated status request,
When `GET /gateway/status` is called,
Then `gateway.web_ui` includes `tools_endpoint` and `tool_stats_endpoint`.

### AC-6 Scoped verification gates pass
Given this implementation slice,
When scoped checks run,
Then `cargo fmt --check`, `cargo clippy -p tau-gateway -- -D warnings`, and targeted gateway tests pass.

## Scope

### In Scope
- Add gateway routes:
  - `GET /gateway/tools`
  - `GET /gateway/tools/stats`
- Aggregate tool usage stats from existing UI telemetry artifacts.
- Add status discovery fields for tools inventory/stats endpoints.
- Add conformance/regression tests for success, fallback, and auth behavior.

### Out of Scope
- Dashboard UI implementation.
- Tool execution runtime redesign.
- New telemetry ingestion producers.

## Conformance Cases
- C-01 (integration): `GET /gateway/tools` returns deterministic authenticated inventory.
- C-02 (integration): `GET /gateway/tools/stats` returns deterministic authenticated aggregate stats.
- C-03 (regression): missing telemetry artifact yields deterministic fallback diagnostics.
- C-04 (regression): malformed telemetry lines are tolerated and counted deterministically.
- C-05 (regression): unauthorized inventory/stats requests return `401`.
- C-06 (regression): `/gateway/status` includes `tools_endpoint` and `tool_stats_endpoint`.
- C-07 (verify): scoped fmt/clippy/targeted tests pass.

## Success Metrics / Observable Signals
- Dashboard can discover and query tool inventory/stats endpoints through `/gateway/status`.
- Tool stats endpoint remains stable under missing/malformed telemetry artifacts.
- Endpoint auth and response contracts are deterministic and test-covered.
