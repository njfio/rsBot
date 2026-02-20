# Spec: Issue #2790 - PRD Phase 1C 14-item sidebar navigation and breadcrumb shell markers

Status: Implemented

## Problem Statement
The current Tau Ops Dashboard shell exposes only a small subset of routes and has no breadcrumb contract. PRD checklist coverage requires stable navigation markers for all operations views and deterministic breadcrumb state for route-aware rendering and test validation.

## Acceptance Criteria

### AC-1 Sidebar renders 14 deterministic ops route links
Given the SSR dashboard shell,
When rendering navigation,
Then output contains 14 route links aligned to PRD view sections under `/ops/*` with stable ids/tokens.

### AC-2 Breadcrumb contract reflects active route context
Given shell context route selection,
When rendering `/ops` vs `/ops/login`,
Then breadcrumb markers expose `home` and current-route labels with deterministic ids/attributes.

### AC-3 Auth shell contracts remain intact with nav expansion
Given auth shell context from Phase 1B,
When navigation/breadcrumb features are added,
Then auth mode and login-required markers remain present and unchanged.

### AC-4 Gateway ops routes continue serving valid shell output
Given gateway route integration,
When requesting `/ops` and `/ops/login`,
Then responses contain expanded navigation and breadcrumb markers without regressing existing behavior.

## Scope

### In Scope
- `tau-dashboard-ui` navigation model and breadcrumb render markers.
- SSR marker tests for 14 links and route/breadcrumb state.
- Gateway integration regression for `/ops` and `/ops/login` shell responses.

### Out of Scope
- Client-side router/hydration route switching.
- Full per-view content implementation behind each navigation item.
- Mobile drawer interactions and hamburger behavior.

## Conformance Cases
- C-01 (conformance): shell includes 14 route links with expected ids/paths.
- C-02 (functional): breadcrumb markers reflect `ops` active route.
- C-03 (functional): breadcrumb markers reflect `login` active route.
- C-04 (regression): auth shell markers remain present after nav expansion.
- C-05 (integration): gateway `/ops` and `/ops/login` responses include nav + breadcrumb markers.

## Success Metrics / Observable Signals
- `cargo test -p tau-dashboard-ui -- --test-threads=1` passes with new nav/breadcrumb assertions.
- `cargo test -p tau-gateway functional_spec_2790 -- --test-threads=1` passes.
- Existing phase-1B route tests remain green.

## Approval Gate
P1 multi-module slice proceeds with spec marked `Reviewed` per AGENTS.md self-acceptance rule.
