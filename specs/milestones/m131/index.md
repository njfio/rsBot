# Milestone M131 - Tau Ops Dashboard PRD Phase 1C (Navigation + Breadcrumb Shell)

Status: InProgress

## Scope
Implement PRD phase-1C shell contracts for navigation fidelity:
- add all 14 sidebar route links derived from PRD view sections,
- render deterministic SSR breadcrumb markers for active ops route context,
- preserve auth/protected shell semantics from Phase 1B.

## Linked Issues
- Epic: #2788
- Story: #2789
- Task: #2790

## Success Signals
- Sidebar contains 14 deterministic route links under `/ops` namespace.
- Breadcrumb marker contract renders predictable route labels and current-route token.
- `/ops` and `/ops/login` route tests remain green.
