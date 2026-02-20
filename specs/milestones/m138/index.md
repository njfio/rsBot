# Milestone M138 - Tau Ops Dashboard PRD Phase 1J (Command Center Alert Feed Contracts)

Status: InProgress

## Scope
Implement command-center alert feed SSR contracts in Tau Ops shell:
- deterministic alert feed list item markers rendered from live dashboard alerts,
- row-level alert metadata contracts (`code`, `severity`, `message`),
- deterministic fallback row markers when no alerts are present.

## Linked Issues
- Epic: #2816
- Story: #2817
- Task: #2818

## Success Signals
- `/ops` shell exposes alert feed list rows derived from dashboard snapshot alerts.
- Each alert row publishes deterministic metadata markers for severity/code/message.
- Empty alert snapshots render a safe fallback marker row.
- Existing phase-1A..1I command-center contract suites remain green.
