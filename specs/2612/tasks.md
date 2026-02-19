# Tasks: Issue #2612 - Runtime log sanitization audit

## Ordered Tasks
1. T1 (RED): add C-01 test asserting tool-audit payloads never serialize raw secret argument/result content.
2. T2 (RED): add C-02 regression test for rate-limit `throttle_principal` redaction of secret-like values.
3. T3 (RED): add C-03 integration test ensuring persisted JSONL lines remain sanitized while preserving expected metadata.
4. T4 (GREEN): implement minimal principal sanitization in `tool_audit_event_json`.
5. T5 (VERIFY): run `cargo fmt --check`, `cargo clippy -p tau-runtime -- -D warnings`, and `cargo test -p tau-runtime`.
6. T6 (CLOSE): update issue status/process log and mark `specs/2612/spec.md` as `Implemented`.

## Tier Mapping
- Unit: C-01
- Property: N/A (no randomized invariants introduced)
- Contract/DbC: N/A (no contract annotations introduced)
- Snapshot: N/A (no snapshot fixtures)
- Functional: C-03
- Conformance: C-01..C-03
- Integration: C-03
- Fuzz: N/A (no new fuzz target surface)
- Mutation: N/A for this targeted logging-hardening slice
- Regression: C-02
- Performance: N/A (no hot-path algorithmic changes)
