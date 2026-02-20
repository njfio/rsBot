# Plan: Issue #2931 - Implement and validate cortex LLM readiness contracts

1. Extend cortex status report schema with readiness fields and supporting telemetry:
   - `health_state`, `rollout_gate`, `reason_code`, `health_reason`
   - `last_event_unix_ms`, `last_event_age_seconds`
2. Add deterministic readiness classification function over observer report data:
   - missing artifact
   - read failure
   - empty artifact
   - malformed lines
   - missing cortex chat activity
   - stale latest event
3. Add live validation utility command under `scripts/dev`:
   - authenticated `/cortex/chat` probe
   - authenticated `/cortex/status` readiness assertion
4. Add/expand gateway tests for new readiness fields and failure-mode reason codes.
5. Run scoped quality/test gates and sanitized live validation evidence.

## Risks / Mitigations
- Risk: readiness thresholds too strict for low-traffic environments.
  - Mitigation: classify stale/no-chat as degraded (not hard failing) unless data is unreadable/malformed.
- Risk: payload schema drift breaking downstream consumers.
  - Mitigation: additive fields only; preserve existing keys and endpoint auth behavior.

## Interface / Contract Notes
- Endpoint path/auth unchanged (`GET /cortex/status`, `POST /cortex/chat`).
- Payload is extended with additive readiness fields.
