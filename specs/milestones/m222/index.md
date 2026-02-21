# M222 - Diagnostics Prompt Telemetry Schema Fail-Closed Enforcement

Status: In Progress

## Context
`tau-diagnostics` currently treats `prompt_telemetry_v1` records without `schema_version` as compatible. This weakens schema enforcement for v1 payloads and can misclassify malformed telemetry as valid.

## Scope
- Require explicit `schema_version == 1` for `prompt_telemetry_v1` compatibility checks.
- Preserve backward compatibility for legacy `prompt_telemetry` (v0) records.
- Add conformance tests for missing-schema v1 behavior.

## Linked Issues
- Epic: #3178
- Story: #3179
- Task: #3180

## Success Signals
- `cargo test -p tau-diagnostics spec_3180 -- --test-threads=1`
- `cargo test -p tau-diagnostics`
- `cargo fmt --check`
- `cargo clippy -p tau-diagnostics -- -D warnings`
