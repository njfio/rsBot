# Milestone M134 - Tau Ops Dashboard PRD Phase 1F (Shell Control Behavior)

Status: InProgress

## Scope
Implement server-driven behavior for Tau Ops shell controls:
- parse route query params for `theme` and `sidebar` shell state,
- apply parsed state to SSR shell context across `/ops*` routes,
- preserve existing auth, breadcrumb, and route-surface contracts.

## Linked Issues
- Epic: #2800
- Story: #2801
- Task: #2802

## Success Signals
- `/ops?theme=light&sidebar=collapsed` renders shell with `data-theme="light"` and `data-sidebar-state="collapsed"`.
- Same query behavior works on secondary ops routes (for example `/ops/chat`, `/ops/agents/default`).
- Existing phase-1B/1C/1D/1E contracts remain green.
