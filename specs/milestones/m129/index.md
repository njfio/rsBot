# Milestone M129 - Tau Ops Dashboard PRD Phase 1A (Leptos Foundation)

Status: InProgress

## Scope
Implement PRD phase-1A foundation for Tau Ops Dashboard:
- bootstrap `tau-dashboard-ui` Leptos crate,
- render an SSR command-center shell,
- integrate gateway route `/ops` to serve the shell.

## Linked Issues
- Epic: #2780
- Story: #2781
- Task: #2782

## Success Signals
- `tau-dashboard-ui` exists in workspace and compiles.
- SSR shell render contract is covered by tests.
- Gateway serves `/ops` shell route with expected marker fields.
