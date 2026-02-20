# Milestone M144 - Tau Ops Dashboard PRD Phase 1P (Session Detail Contracts)

Status: InProgress

## Scope
Implement Tau Ops `/ops/sessions/{session_key}` deterministic session detail contracts:
- dedicated session detail panel SSR markers for selected session key,
- deterministic message timeline row markers sourced from session lineage,
- explicit validation-report and usage-summary SSR markers sourced from session store state.

## Linked Issues
- Epic: #2840
- Story: #2841
- Task: #2842

## Success Signals
- `/ops/sessions/{session_key}` HTML includes deterministic detail panel/list/timeline markers.
- Validation report markers reflect `SessionValidationReport` values for selected session.
- Usage summary markers reflect `SessionUsageSummary` values for selected session.
- Existing `/ops/sessions` list and `/ops/chat` contracts remain green.
