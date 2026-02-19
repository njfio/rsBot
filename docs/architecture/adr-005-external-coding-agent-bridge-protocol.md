# ADR-005: Staged External Coding-Agent Bridge Protocol Runtime

## Context

Gap G21 requires delegated external coding-agent workflows with session lifecycle controls, progress streaming, interactive follow-ups, and inactivity cleanup. Tau previously had no dedicated bridge/session-pool contract for this flow.

## Decision

Add an additive runtime module (`tau-runtime::external_coding_agent_bridge_runtime`) that provides:

1. Session pool lifecycle APIs (`open_or_reuse_session`, terminal state markers, close).
2. Ordered progress/follow-up event stream replay (`poll_events` with monotonic sequence IDs).
3. Follow-up queue support (`queue_followup`, `take_followups`).
4. Deterministic inactivity cleanup (`reap_inactive_sessions(now_unix_ms)`) with default 10-minute timeout.

The module is protocol/runtime staging only. It does not spawn subprocesses or expose HTTP/SSE endpoints directly in this phase.

## Consequences

### Positive
- Creates explicit lifecycle and stream contracts for future gateway/worker adapters.
- Supports deterministic testing of timeout/cleanup behavior.
- Keeps current runtime behavior unchanged unless bridge APIs are used.

### Negative
- Additional API surface requires versioning discipline as full integration lands.
- Real subprocess orchestration remains follow-up work.

### Follow-up
- Wire bridge sessions to concrete subprocess/HTTP+SSE adapters.
- Integrate adapter-level auth/safety policy controls and workspace-level quotas.
