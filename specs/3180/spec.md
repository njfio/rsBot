# Spec: Issue #3180 - enforce prompt_telemetry_v1 schema-version requirement

Status: Accepted

## Problem Statement
The diagnostics summarizer currently accepts `prompt_telemetry_v1` records with missing `schema_version`. That violates v1 schema strictness and can count malformed telemetry as valid provider records.

## Scope
In scope:
- Require `schema_version == PROMPT_TELEMETRY_SCHEMA_VERSION` for `prompt_telemetry_v1` compatibility.
- Add conformance tests proving v1 records missing schema version are ignored.
- Preserve legacy `prompt_telemetry` compatibility semantics.

Out of scope:
- New record types.
- Telemetry emission changes in other crates.
- Schema version bumps.

## Acceptance Criteria
### AC-1 v1 records missing schema_version are not counted as compatible prompt telemetry
Given audit JSONL containing `record_type: prompt_telemetry_v1` without `schema_version`,
when summarized,
then provider prompt counters do not include those malformed records.

### AC-2 Legacy prompt telemetry compatibility remains intact
Given audit JSONL containing legacy `record_type: prompt_telemetry` entries,
when summarized,
then those records remain compatible under current legacy behavior.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional/Conformance | one v1 record missing schema_version + one valid tool event | summarize audit file | prompt count remains 0, tool count remains 1 |
| C-02 | AC-2 | Functional/Conformance | one legacy prompt telemetry record without schema_version | summarize audit file | legacy record is counted as prompt telemetry |

## Success Metrics / Observable Signals
- `cargo test -p tau-diagnostics spec_3180 -- --test-threads=1`
- `cargo test -p tau-diagnostics`
- `cargo fmt --check`
- `cargo clippy -p tau-diagnostics -- -D warnings`
