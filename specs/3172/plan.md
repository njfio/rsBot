# Plan: Issue #3172 - training-proxy JSONL newline delimiter integrity

## Approach
1. Add RED conformance test for missing trailing newline append scenario (C-01).
2. Reuse existing append conformance test as C-02 coverage.
3. Implement minimal fix in attribution append function to insert delimiter newline when needed.
4. Re-run targeted and crate-level verification checks.

## Affected Modules
- `crates/tau-training-proxy/src/lib.rs`
- `specs/milestones/m220/index.md`
- `specs/3172/spec.md`
- `specs/3172/plan.md`
- `specs/3172/tasks.md`

## Risks & Mitigations
- Risk: extra I/O overhead in append path.
  - Mitigation: only perform file content check when file exists and has non-zero length.
- Risk: accidental alteration of existing successful append flow.
  - Mitigation: keep changes minimal and preserve prior append semantics for normal files.

## Interfaces / Contracts
- Attribution JSONL format contract: one JSON object per line.
- Append behavior in `append_attribution_record`.

## ADR
No ADR required (single-module persistence behavior hardening).
