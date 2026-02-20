# Spec: Issue #2794 - PRD Phase 1D gateway route coverage for 14 ops sidebar destinations

Status: Implemented

## Problem Statement
The dashboard shell now renders 14 PRD sidebar links, but gateway currently serves only `/ops` and `/ops/login`. Most sidebar destinations return 404, so navigation cannot be validated end-to-end and breadcrumb/active-route contracts are not exercised on real routes.

## Acceptance Criteria

### AC-1 Gateway registers routes for all 14 sidebar destinations
Given the ops sidebar route set,
When requesting each `/ops/*` destination path,
Then gateway returns `200 OK` HTML shell output for all 14 destinations.

### AC-2 Route-specific shell context updates active-route markers
Given requests to each ops route,
When shell is rendered,
Then `data-active-route` matches the intended route context token.

### AC-3 Breadcrumb marker tracks route context across route set
Given requests to each ops route,
When shell is rendered,
Then `data-breadcrumb-current` marker maps to route-appropriate breadcrumb token.

### AC-4 Prior auth/bootstrap and dashboard behavior remains stable
Given existing phase-1B/1C contracts,
When phase-1D route coverage is integrated,
Then auth bootstrap endpoint, auth session endpoint, and legacy dashboard shell tests remain green.

## Scope

### In Scope
- Gateway route constants and router wiring for 14 ops destinations.
- Route-context-aware handlers mapping to `tau-dashboard-ui` shell context.
- Integration tests validating status + active-route/breadcrumb markers across all routes.

### Out of Scope
- Full per-view domain logic/data loading for each route.
- Client-side router/hydration transitions between routes.
- Authorization policy changes beyond current auth bootstrap/session behavior.

## Conformance Cases
- C-01 (integration): all 14 sidebar route paths return `200` HTML shell.
- C-02 (functional): `data-active-route` markers match expected per-path route token.
- C-03 (functional): `data-breadcrumb-current` markers match expected per-path breadcrumb token.
- C-04 (regression): `/gateway/auth/bootstrap` and `POST /gateway/auth/session` behavior unchanged.
- C-05 (regression): `/dashboard` legacy shell behavior unchanged.

## Success Metrics / Observable Signals
- `cargo test -p tau-gateway functional_spec_2794 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2786 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_dashboard_shell_endpoint_returns_html_shell -- --test-threads=1` passes.

## Approval Gate
P1 multi-module slice proceeds with spec marked `Reviewed` per AGENTS.md self-acceptance rule.
