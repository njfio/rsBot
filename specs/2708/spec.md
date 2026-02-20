# Spec: Issue #2708 - Cortex observer coverage for memory-save and worker-progress signals

Status: Implemented

## Problem Statement
`/cortex/status` currently tracks a narrow set of gateway events (chat/session append/reset and external coding session open/close). Remaining G3 tracking goals in `tasks/spacebot-comparison.md` require broader runtime visibility, especially around memory-save operations and worker/session progress signals.

## Acceptance Criteria

### AC-1 Memory-save gateway operations emit Cortex observer events
Given authenticated valid memory-save requests,
When gateway memory write/update/delete endpoints are called,
Then Cortex observer persistence records deterministic event types for those operations.

### AC-2 Worker/session progress operations emit Cortex observer events
Given authenticated valid external coding worker progress/followup requests,
When progress/followup endpoints are called,
Then Cortex observer persistence records deterministic event types for those operations.

### AC-3 Cortex status reflects expanded event counters
Given the expanded operations are invoked,
When authenticated `GET /cortex/status` is called,
Then `event_type_counts` and `total_events` include the new event classes.

### AC-4 Existing auth and fallback contracts remain intact
Given missing auth or missing observer artifacts,
When `GET /cortex/status` is called,
Then unauthorized requests return `401` and missing-state requests return deterministic `200` fallback payload.

### AC-5 Scoped verification gates pass
Given this implementation slice,
When scoped checks run,
Then `cargo fmt --check`, `cargo clippy -p tau-gateway -- -D warnings`, and targeted gateway tests pass.

## Scope

### In Scope
- Add Cortex observer event tracking for:
  - `memory.write`
  - `memory.entry_write`
  - `memory.entry_delete`
  - `external_coding_agent.progress`
  - `external_coding_agent.followup_queued`
- Extend conformance/regression tests to validate expanded event counters through `/cortex/status`.
- Preserve fail-open telemetry writes and existing status endpoint contracts.

### Out of Scope
- Cortex bulletin generation and prompt injection.
- Cross-process model routing decisions.
- New UI work.

## Conformance Cases
- C-01 (integration): memory write/update/delete operations increment corresponding Cortex event counters.
- C-02 (integration): external coding progress/followup operations increment corresponding Cortex event counters.
- C-03 (integration): `/cortex/status` returns expanded deterministic counters and total event count.
- C-04 (regression): unauthorized `/cortex/status` remains `401`.
- C-05 (regression): missing observer artifacts still return deterministic fallback payload.
- C-06 (verify): scoped fmt/clippy/targeted tests pass.

## Success Metrics / Observable Signals
- Operators can inspect memory-save and worker-progress activity via `/cortex/status` without relying on separate logs.
- Existing Cortex status auth/fallback behavior remains stable.
- Expanded tracking is conformance-backed and CI green.
