# Spec: Issue #2798 - PRD Phase 1E responsive sidebar and theme shell controls

Status: Implemented

## Problem Statement
PRD foundation checklist requires responsive sidebar behavior (mobile collapse + hamburger toggle) and dark/light theme switching, but the current SSR shell only exposes static nav/auth/content markers. There is no explicit contract surface for responsive controls or theme state.

## Acceptance Criteria

### AC-1 Shell exposes responsive sidebar policy + control markers
Given `/ops` shell rendering,
When SSR HTML is generated,
Then shell includes explicit mobile-sidebar policy markers and sidebar control IDs/attributes needed for responsive collapse behavior.

### AC-2 Hamburger control contract supports sidebar open/close state signaling
Given shell rendering for sidebar expanded/collapsed states,
When SSR HTML is generated,
Then hamburger control markers (`aria-controls`, `aria-expanded`, toggle target/state markers) map correctly to sidebar state.

### AC-3 Theme toggle contract supports dark/light state signaling
Given shell rendering for dark/light themes,
When SSR HTML is generated,
Then shell root and theme controls expose deterministic markers identifying active theme and available toggle targets.

### AC-4 Existing phase-1B/1C/1D contracts remain stable
Given prior auth/bootstrap/nav/route contracts,
When phase-1E responsive/theme controls are integrated,
Then existing tests for auth bootstrap/session, route markers, and breadcrumbs remain green.

## Scope

### In Scope
- SSR shell contract markers for responsive sidebar + hamburger controls.
- SSR shell contract markers for dark/light theme controls.
- Shell context expansion for sidebar state/theme state signaling.
- Regression test coverage for existing route/auth contracts.

### Out of Scope
- Full hydrated client-side animation/state machine implementation.
- Complete CSS design system/theming tokens beyond contract-level foundation markers.
- New gateway domain APIs.

## Conformance Cases
- C-01 (functional): `/ops` shell includes responsive sidebar policy/control markers.
- C-02 (unit/functional): sidebar expanded vs collapsed shell states set correct toggle/shell markers.
- C-03 (unit/functional): dark vs light shell states set correct root/theme control markers.
- C-04 (integration): `/ops` gateway shell output includes phase-1E responsive/theme contract markers.
- C-05 (regression): phase-1B/1C/1D tests remain green.

## Success Metrics / Observable Signals
- `cargo test -p tau-dashboard-ui functional_spec_2798 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2798 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2786 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2794 -- --test-threads=1` passes.

## Approval Gate
P1 multi-module slice proceeds with spec marked `Reviewed` per AGENTS.md self-acceptance rule.
