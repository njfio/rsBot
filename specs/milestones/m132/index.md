# Milestone M132 - Tau Ops Dashboard PRD Phase 1D (Ops Route Surface)

Status: InProgress

## Scope
Implement PRD phase-1D route-surface contracts:
- gateway route registration for all 14 ops sidebar destinations,
- route-specific SSR shell context so active-route and breadcrumb markers align with requested path,
- regression preservation for existing auth bootstrap/session and legacy dashboard routes.

## Linked Issues
- Epic: #2792
- Story: #2793
- Task: #2794

## Success Signals
- Every sidebar path under `/ops/*` returns `200` with shell markers.
- `data-active-route` and `data-breadcrumb-current` match route context for each destination.
- Existing phase-1B and phase-1C tests remain green.
