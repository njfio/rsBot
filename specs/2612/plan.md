# Plan: Issue #2612 - Runtime log sanitization audit

## Approach
1. Add RED tests in `tau-runtime` observability logger module to assert secret fixtures never appear in tool-audit payloads/log lines.
2. Implement minimal deterministic sanitizer for `throttle_principal` metadata in tool-audit events.
3. Re-run targeted runtime tests and scoped verification gates.
4. Publish issue/process evidence with AC -> test mapping.

## Affected Modules
- `crates/tau-runtime/src/observability_loggers_runtime.rs`
- `specs/2612/spec.md`
- `specs/2612/plan.md`
- `specs/2612/tasks.md`
- `specs/milestones/m104/index.md`

## Risks / Mitigations
- Risk: Over-redaction could remove useful non-secret metadata.
  - Mitigation: redact only secret-like principal patterns and preserve safe principal values.
- Risk: Tests may become brittle on incidental formatting.
  - Mitigation: assert on semantic JSON fields and absence/presence of deterministic substrings.
- Risk: Logger changes could affect existing telemetry tests.
  - Mitigation: keep changes constrained to principal sanitization path and rerun full `tau-runtime` test suite.

## Interfaces / Contracts
- `tool_audit_event_json(event, starts)` output contract:
  - Keeps `arguments_bytes`/`result_bytes` only for content fields.
  - Emits sanitized `throttle_principal` when rate-limit metadata is present.
- `ToolAuditLogger::log_event` JSONL persistence contract remains stable.

## ADR
- Not required: no dependency additions or protocol/architecture changes.
