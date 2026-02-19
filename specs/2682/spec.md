# Spec: Issue #2682 - PRD gateway audit summary and audit log endpoints

Status: Implemented

## Problem Statement
`specs/tau-ops-dashboard-prd.md` requires diagnostics APIs for aggregated audit metrics and paginated audit record retrieval (`/gateway/audit/summary`, `/gateway/audit/log`). `tau-gateway` currently persists dashboard action audit records and UI telemetry events, but does not expose API endpoints that let operators query those records for dashboard diagnostics workflows.

## Acceptance Criteria

### AC-1 Authenticated operators can fetch audit summary aggregates
Given a valid authenticated request,
When `GET /gateway/audit/summary` is called,
Then the gateway returns a deterministic summary payload that aggregates dashboard action audit and UI telemetry records, including totals, per-source counts, and reason/action/view counters.

### AC-2 Audit summary supports time-window filtering
Given a valid authenticated request with `since_unix_ms` and/or `until_unix_ms` query filters,
When `GET /gateway/audit/summary` is called,
Then only records within the requested window are counted.

### AC-3 Authenticated operators can fetch paginated audit records
Given a valid authenticated request,
When `GET /gateway/audit/log` is called,
Then the gateway returns merged audit records sorted by timestamp (newest first) with deterministic pagination metadata.

### AC-4 Audit log supports deterministic source/action/view/reason/time filters
Given a valid authenticated request with supported filters,
When `GET /gateway/audit/log` is called,
Then only matching records are returned and pagination metadata reflects filtered totals.

### AC-5 Invalid audit query inputs fail closed
Given invalid query inputs (unsupported source, invalid bounds, non-positive page/page_size, oversized page_size),
When audit summary/log endpoints are called,
Then the gateway returns `400` with deterministic error codes.

### AC-6 Unauthorized audit requests are rejected
Given missing or invalid auth,
When audit summary/log endpoints are called,
Then the gateway returns fail-closed `401`.

### AC-7 Gateway status discovery advertises audit endpoints
Given an authenticated status request,
When `GET /gateway/status` is called,
Then `gateway.web_ui` includes `audit_summary_endpoint` and `audit_log_endpoint`.

### AC-8 Scoped verification gates pass
Given this implementation slice,
When scoped checks run,
Then `cargo fmt --check`, `cargo clippy -p tau-gateway -- -D warnings`, and targeted gateway tests pass.

## Scope

### In Scope
- Add new gateway route templates:
  - `GET /gateway/audit/summary`
  - `GET /gateway/audit/log`
- Parse dashboard action audit records (`.tau/dashboard/actions-audit.jsonl`) and UI telemetry records (`.tau/gateway/openresponses/ui-telemetry.jsonl`).
- Merge, filter, and paginate audit records for API responses.
- Provide summary aggregates and invalid-record counters.
- Add status discovery fields for new endpoints.
- Add integration/regression tests for auth, filtering, pagination, validation, and discovery.

### Out of Scope
- New audit persistence backends or schema migrations.
- Dashboard frontend rendering changes.
- Historical export/download workflows beyond response pagination.

## Conformance Cases
- C-01 (functional): `GET /gateway/audit/summary` returns merged totals and per-source aggregates.
- C-02 (functional): `GET /gateway/audit/summary` applies time-window filters deterministically.
- C-03 (regression): malformed audit log lines do not crash handlers and are surfaced as invalid-record counts.
- C-04 (functional): `GET /gateway/audit/log` returns merged newest-first records with stable pagination metadata.
- C-05 (functional): `GET /gateway/audit/log` applies source/action/view/reason/time filters.
- C-06 (regression): invalid audit query inputs return deterministic `400` errors.
- C-07 (regression): unauthorized audit summary/log requests return `401`.
- C-08 (regression): `GET /gateway/status` includes `audit_summary_endpoint` and `audit_log_endpoint`.
- C-09 (verify): scoped fmt/clippy/targeted tests pass.

## Success Metrics / Observable Signals
- Operators can query audit metrics and detailed records through authenticated gateway APIs without reading files directly.
- Query validation fails closed with deterministic error codes.
- Endpoint discovery for diagnostics surfaces is fully API-driven via `/gateway/status`.
