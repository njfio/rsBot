# Milestone M147 - Tau Ops Dashboard PRD Phase 1S (Command-Center Route Visibility Contracts)

Status: InProgress

## Scope
Implement deterministic command-center route visibility contracts:
- command-center panel is visible on `/ops`,
- command-center panel is hidden on non-command-center ops routes,
- existing command-center/chart/control marker contracts remain stable.

## Linked Issues
- Epic: #2852
- Story: #2853
- Task: #2854

## Success Signals
- `/ops` shell reports command-center panel visible via deterministic markers.
- non-command-center routes (`/ops/chat`, `/ops/sessions`) report command-center panel hidden via deterministic markers.
- Existing command-center and route-specific panel contracts remain green.
