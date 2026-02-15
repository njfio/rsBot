# Issue 1744 Plan

Status: Reviewed

## Approach

1. Add lifecycle control audit schema helpers in `tau-runtime` with a stable
   v1 record type and required fields.
2. Extend RPC serve flow with an audit-enabled variant that emits lifecycle
   control transition records to a separate JSONL writer while preserving
   existing serve output behavior.
3. Extend `tau-diagnostics::summarize_audit_file` to validate lifecycle control
   audit records and report compliant/non-compliant counts.
4. Add tests for schema conformance, serve transition logging, and malformed
   record compliance detection.

## Affected Areas

- `crates/tau-runtime/src/rpc_protocol_runtime.rs`
- `crates/tau-runtime/src/lib.rs`
- `crates/tau-diagnostics/src/lib.rs`
- `specs/1744/{spec,plan,tasks}.md`

## Risks And Mitigations

- Risk: introducing lifecycle audit emission could alter existing serve output.
  - Mitigation: add a separate audit-enabled serve entrypoint and keep existing
    serve API unchanged.
- Risk: false-positive compliance failures on mixed audit files.
  - Mitigation: validate only lifecycle control record type and ignore unrelated
    records.

## ADR

No architecture/dependency/protocol change. ADR not required.
