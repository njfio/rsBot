# Milestone M139 - Tau Ops Dashboard PRD Phase 1K (Command Center Connector Health Contracts)

Status: InProgress

## Scope
Implement command-center connector health table SSR contracts in Tau Ops shell:
- deterministic connector row markers rendered from live multi-channel connector state,
- per-row connector metadata contracts (`channel`, `mode`, `liveness`, counters),
- deterministic fallback row markers when connector state is unavailable.

## Linked Issues
- Epic: #2820
- Story: #2821
- Task: #2822

## Success Signals
- `/ops` shell exposes connector health rows backed by multi-channel connector state.
- Each connector row publishes deterministic metadata markers for channel/mode/liveness/counters.
- Missing connector state renders a safe fallback connector row.
- Existing phase-1A..1J command-center suites remain green.
