# Plan: Issue #3164 - tau-training-proxy malformed-header and attribution-log resilience conformance

## Approach
1. Add spec-mapped RED tests for C-01..C-04 in `crates/tau-training-proxy/src/lib.rs`.
2. Run targeted tests to capture RED evidence (at least one failing conformance test).
3. Implement minimal recovery fix in attribution append path (recreate parent directory before open).
4. Re-run targeted tests to GREEN, then scoped crate validation (`fmt`, `clippy`, crate tests).

## Affected Modules
- `crates/tau-training-proxy/src/lib.rs`
- `specs/3164/spec.md`
- `specs/3164/plan.md`
- `specs/3164/tasks.md`
- `specs/milestones/m218/index.md`

## Risks & Mitigations
- Risk: brittle integration assertions over JSONL payload shape.
  - Mitigation: assert only stable key fragments and line counts.
- Risk: accidental behavior changes in proxy response path.
  - Mitigation: keep implementation scope limited to attribution log writer path.

## Interfaces / Contracts
- `parse_training_proxy_attribution` header parsing error behavior.
- Attribution log append contract at `state_dir/training/proxy-attribution.jsonl`.

## ADR
No ADR required (single-module, non-architectural behavior hardening).
