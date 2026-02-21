# Plan: Issue #3180 - enforce prompt_telemetry_v1 schema-version requirement

## Approach
1. Add RED conformance tests for C-01/C-02 in `crates/tau-diagnostics/src/lib.rs`.
2. Run targeted tests to capture RED failure on current permissive v1 missing-schema behavior.
3. Apply minimal compatibility predicate fix in `is_compatible_prompt_telemetry_record`.
4. Re-run targeted tests to GREEN and execute crate verification checks.

## Affected Modules
- `crates/tau-diagnostics/src/lib.rs`
- `specs/milestones/m222/index.md`
- `specs/3180/spec.md`
- `specs/3180/plan.md`
- `specs/3180/tasks.md`

## Risks & Mitigations
- Risk: stricter parsing may drop older malformed v1 logs.
  - Mitigation: retain explicit legacy v0 compatibility path; strictness applies only to v1-tagged records.
- Risk: test brittleness on aggregate counters.
  - Mitigation: assert on stable count fields only.

## Interfaces / Contracts
- `is_compatible_prompt_telemetry_record` compatibility policy.
- `summarize_audit_file` prompt and tool aggregate counters.

## ADR
No ADR required (single-module validation strictness correction).
