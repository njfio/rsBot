# Issue 1724 Plan

Status: Reviewed

## Approach

1. Extend `tau-training-types` RL schema tests with legacy decode fixtures
   (missing `schema_version`) for trajectory, advantage, and checkpoint records.
2. Add unknown-version regression tests for all RL schema structs and assert
   deterministic error strings.
3. Add concise migration guarantee comments near schema-version constants and/or
   tests to keep contract intent visible.

## Affected Areas

- `crates/tau-training-types/src/lib.rs`
- `specs/1724/{spec,plan,tasks}.md`

## Risks And Mitigations

- Risk: brittle string assertions on errors.
  - Mitigation: assert stable core fragments (`unsupported schema version` and
    type names).
- Risk: partial coverage across RL structs.
  - Mitigation: include fixtures for trajectory, advantage batch, and checkpoint.

## ADR

No architecture/dependency/protocol change. ADR not required.
