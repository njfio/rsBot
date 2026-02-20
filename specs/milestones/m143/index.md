# Milestone M143 - Tau Ops Dashboard PRD Phase 1O (Sessions Explorer List Contracts)

Status: InProgress

## Scope
Implement Tau Ops `/ops/sessions` deterministic session explorer contracts:
- dedicated sessions panel SSR markers,
- deterministic session row markers sourced from discovered gateway sessions,
- explicit empty-state marker behavior when no session files exist.

## Linked Issues
- Epic: #2836
- Story: #2837
- Task: #2838

## Success Signals
- `/ops/sessions` HTML includes deterministic panel/list/row markers.
- Session rows map from discovered gateway session files and expose deterministic chat-route links.
- Empty-state marker renders when the sessions store has no files.
- Existing ops-shell route and chat contracts remain green.
