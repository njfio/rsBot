# Plan: Issue #3052 - Diagnostics telemetry aggregation hardening

## Approach
1. Add RED conformance tests for total-token fallback and mixed-record aggregation behavior in `tau-diagnostics`.
2. Implement minimal runtime fallback aggregation when `total_tokens` is absent.
3. Re-run targeted and full crate tests plus formatting/lint/check gates.

## Affected Paths
- `crates/tau-diagnostics/src/lib.rs`
- `specs/milestones/m191/index.md`
- `specs/3052/spec.md`
- `specs/3052/plan.md`
- `specs/3052/tasks.md`

## Risks and Mitigations
- Risk: changing aggregation semantics for existing records.
  - Mitigation: restrict fallback to only cases where `total_tokens` is missing; preserve explicit values when present.
- Risk: brittle test fixtures.
  - Mitigation: use deterministic in-crate JSONL fixtures with precise token assertions.

## Interfaces / Contracts
- Prompt telemetry aggregation computes `total_tokens` from `input_tokens + output_tokens` when `total_tokens` is absent.
- Existing explicit `total_tokens` behavior remains unchanged.

## ADR
Not required (localized compatibility hardening in diagnostics aggregation path).
