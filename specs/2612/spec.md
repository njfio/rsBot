# Spec: Issue #2612 - Runtime log sanitization audit and leak-prevention checks

Status: Implemented

## Problem Statement
Runtime observability logs must never emit credential/token material. Existing logger coverage validates structure and counters but does not explicitly enforce redaction guarantees for secret-like values flowing through tool-event metadata. This creates false-negative risk where high-entropy secrets could leak into persisted logs.

## Acceptance Criteria

### AC-1 Tool audit payloads remain content-redacted for tool arguments/results
Given `tool_audit_event_json` receives tool start/end events containing secret-like argument/result strings,
When payload JSON is generated,
Then the payload omits raw argument/result content and contains only size/metadata fields.

### AC-2 Throttle principal metadata is sanitized fail-closed
Given a rate-limit tool result includes a secret-like `principal` value,
When tool audit payload metadata is emitted,
Then `throttle_principal` is redacted to a deterministic sentinel value instead of logging raw secret material.

### AC-3 Persisted audit logs satisfy deterministic leak-prevention checks
Given `ToolAuditLogger` writes JSONL events,
When secret-like inputs are present in runtime events,
Then persisted log lines do not contain raw secret/token fixtures and retain expected non-secret metadata.

### AC-4 Scoped verification gates are green
Given new sanitization logic/tests,
When scoped checks run for `tau-runtime`,
Then fmt, clippy, and crate tests pass.

## Scope

### In Scope
- `crates/tau-runtime/src/observability_loggers_runtime.rs` log sanitization behavior.
- New unit/integration/regression tests validating deterministic redaction guarantees.
- M104 milestone + issue spec artifacts for this slice.

### Out of Scope
- Changing secret-detection regex packs in `tau-safety`.
- New runtime dependencies.
- Broader dashboard/gateway logging format redesign.

## Conformance Cases
- C-01 (AC-1, unit): `unit_spec_2612_c01_tool_audit_payload_omits_secret_argument_and_result_content`
- C-02 (AC-2, regression): `regression_spec_2612_c02_tool_audit_redacts_secret_like_throttle_principal`
- C-03 (AC-3, integration): `integration_spec_2612_c03_tool_audit_logger_persists_redacted_principal_without_secret_literals`
- C-04 (AC-4, verify): scoped lint/test commands in tasks.

## Success Metrics / Observable Signals
- C-01..C-03 pass in `tau-runtime`.
- Scoped gates pass:
  - `cargo fmt --check`
  - `cargo clippy -p tau-runtime -- -D warnings`
  - `cargo test -p tau-runtime`
