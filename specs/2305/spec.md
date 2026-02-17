# Spec #2305

Status: Implemented
Milestone: specs/milestones/m48/index.md
Issue: https://github.com/njfio/Tau/issues/2305

## Problem Statement

Gateway OpenResponses requests collect turn usage in-memory and return usage in API responses, but session-level usage totals are not persisted in `tau-session` for gateway-backed sessions. As a result, cumulative token/cost tracking for a gateway session is lost between requests and reloads.

## Scope

In scope:

- Persist per-request usage/cost deltas for OpenResponses gateway requests into `SessionStore` usage summaries.
- Ensure persisted usage totals are cumulative across repeated requests to the same session key.
- Preserve existing OpenResponses HTTP/SSE response schema and behavior.
- Add conformance and regression tests validating persisted usage rollups.

Out of scope:

- Gateway response schema changes.
- Cross-session or organization-level billing exports.
- Provider pricing catalog changes.

## Acceptance Criteria

- AC-1: Given a successful OpenResponses request with a gateway session key, when the session store is reloaded, then `usage_summary` reflects non-zero token totals for that request.
- AC-2: Given multiple successful OpenResponses requests against the same session key, when usage is reloaded, then totals equal the sum of per-response usage values.
- AC-3: Given usage persistence is added, when existing OpenResponses responses are returned, then response schema remains unchanged (no required new fields).

## Conformance Cases

- C-01 (AC-1, integration): One `/v1/responses` call persists usage summary totals in session storage.
- C-02 (AC-2, integration): Two `/v1/responses` calls for the same session persist cumulative totals equal to response usage sums.
- C-03 (AC-3, regression): Existing OpenResponses endpoint tests for response envelope remain green without payload-shape updates.

## Success Metrics / Observable Signals

- Gateway session stores include non-default `usage_summary` after OpenResponses execution.
- Reloaded `SessionStore` for a gateway session preserves usage totals across process boundaries.
- Existing OpenResponses compatibility tests continue to pass unchanged.
