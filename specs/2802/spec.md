# Spec: Issue #2802 - Query-driven shell control behavior for theme and sidebar state

Status: Implemented

## Problem Statement
Phase 1E introduced shell control markers and links, but the links do not currently alter rendered shell state because `/ops*` handlers ignore `theme` and `sidebar` query params. Dark/light toggle and sidebar expand/collapse controls are therefore non-functional.

## Acceptance Criteria

### AC-1 `/ops` parses and applies query-driven shell control state
Given a request to `/ops` with `theme` and `sidebar` query params,
When SSR shell is rendered,
Then shell markers reflect requested state (`data-theme`, `data-sidebar-state`, toggle aria markers).

### AC-2 Query-driven shell control state applies across routed `/ops*` views
Given requests to routed ops views (for example `/ops/chat`, `/ops/agents/default`) with query params,
When SSR shell is rendered,
Then control state markers reflect requested theme/sidebar state while preserving route markers.

### AC-3 Invalid query values degrade safely to defaults
Given unsupported values for `theme` or `sidebar`,
When SSR shell is rendered,
Then shell falls back to default control state (`dark`, `expanded`) without panics.

### AC-4 Existing phase-1B/1C/1D/1E contracts remain stable
Given existing auth, breadcrumb, route-surface, and control-marker tests,
When query-driven behavior is integrated,
Then those tests remain green.

## Scope

### In Scope
- Query parsing and normalization for `theme` and `sidebar`.
- Handler integration so parsed values populate `TauOpsDashboardShellContext`.
- Functional/integration tests for valid and invalid query values.

### Out of Scope
- Client-side animation/interactivity beyond server-rendered state.
- Persisting control state in cookies/local storage.
- Visual redesign of control UI.

## Conformance Cases
- C-01 (integration): `/ops?theme=light&sidebar=collapsed` returns shell with corresponding markers.
- C-02 (integration): `/ops/chat?theme=light&sidebar=collapsed` and `/ops/agents/default?...` preserve route marker + control markers.
- C-03 (integration): invalid query values fall back to default markers.
- C-04 (regression): phase-1B/1C/1D/1E test suites remain green.

## Success Metrics / Observable Signals
- `cargo test -p tau-gateway functional_spec_2802 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2786 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2794 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2798 -- --test-threads=1` passes.

## Approval Gate
P1 multi-module slice proceeds with spec marked `Reviewed` per AGENTS.md self-acceptance rule.
