# Milestone M135 - Tau Ops Dashboard PRD Phase 1G (Command Center Live Data Contracts)

Status: InProgress

## Scope
Implement command-center live-data SSR contracts in Tau Ops shell:
- health badge markers bound to dashboard snapshot health state/reason,
- six KPI stat-card markers bound to live snapshot values,
- alerts feed markers bound to live alert payloads,
- queue timeline markers bound to live timeline snapshot metadata.

## Linked Issues
- Epic: #2804
- Story: #2805
- Task: #2806

## Success Signals
- `/ops` shell renders health/KPI/alerts/timeline markers from dashboard snapshot fixtures.
- Marker contracts remain deterministic and testable in SSR output.
- Existing phase-1B/1C/1D/1E/1F contracts remain green.
