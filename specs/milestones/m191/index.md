# M191 - Diagnostics Telemetry Aggregation Hardening

## Context
`tau-diagnostics` summarizes prompt telemetry JSONL records, but compatibility payloads can omit `total_tokens` while still providing `input_tokens` and `output_tokens`. Aggregation should remain deterministic across these variants.

## Scope
- Add conformance tests for total-token fallback aggregation behavior.
- Implement minimal runtime fix if fallback aggregation is missing.
- Keep changes scoped to `crates/tau-diagnostics` and issue-linked spec artifacts.

## Linked Issues
- Epic: #3050
- Story: #3051
- Task: #3052
