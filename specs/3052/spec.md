# Spec: Issue #3052 - Diagnostics total-token fallback aggregation hardening

Status: Reviewed

## Problem Statement
`summarize_audit_file` aggregates provider token usage from prompt telemetry records. Some compatibility payloads emit `input_tokens`/`output_tokens` without `total_tokens`. Current behavior must remain deterministic and avoid undercounting total usage in such cases.

## Acceptance Criteria

### AC-1 Fallback total-token aggregation
Given a compatible prompt telemetry record with `input_tokens` and `output_tokens` but missing `total_tokens`,
When `summarize_audit_file` aggregates provider usage,
Then `total_tokens` is computed as `input_tokens + output_tokens`.

### AC-2 Mixed-record aggregation remains correct
Given a provider aggregate across records with and without explicit `total_tokens`,
When `summarize_audit_file` processes the audit file,
Then aggregate token counters are deterministic and additive.

### AC-3 Verification gates remain green
Given the test/runtime updates,
When running validation,
Then targeted crate tests plus fmt/clippy/check pass.

## Scope

### In Scope
- `crates/tau-diagnostics/src/lib.rs`
- `specs/milestones/m191/index.md`
- `specs/3052/*`

### Out of Scope
- New diagnostics record types.
- Changes to audit file format emitters outside diagnostics aggregation.

## Conformance Cases
- C-01 (AC-1): missing `total_tokens` falls back to `input+output`.
- C-02 (AC-2): mixed explicit/fallback records aggregate deterministically for one provider.
- C-03 (AC-3): validation command set passes.

## Success Metrics / Observable Signals
- `cargo test -p tau-diagnostics spec_c01_summarize_audit_file_falls_back_total_tokens_when_missing -- --nocapture`
- `cargo test -p tau-diagnostics spec_c02_summarize_audit_file_mixed_total_token_records_aggregate_deterministically -- --nocapture`
- `cargo test -p tau-diagnostics -- --nocapture --test-threads=1`
- `cargo fmt --check`
- `cargo clippy -p tau-diagnostics -- -D warnings`
- `cargo check -q`

## Approval Gate
P1 scope: spec authored/reviewed by agent; implementation proceeds and is flagged for human review in PR.
