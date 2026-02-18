# Plan #2444

Status: Reviewed
Spec: specs/2444/spec.md

## Approach

1. Add relation storage support in memory persistence layer with migration-safe
   table creation.
2. Extend memory write args + validation for explicit relation attachment.
3. Expose relation data in read/search outputs.
4. Integrate graph contribution into ranking pipeline as additive weighted
   signal while preserving deterministic ordering.
5. Add conformance/regression tests first, then implementation.

## Affected Modules (planned)

- `crates/tau-memory/src/` (schema, relation storage/query, ranking)
- `crates/tau-tools/src/tools/memory_tools.rs` (write/read/search contract)
- `crates/tau-tools/src/tools/tests.rs` and/or `tau-memory` tests

## Risks and Mitigations

- Risk: ranking regressions from graph signal over-weighting.
  - Mitigation: cap graph contribution and test tie-break behavior.
- Risk: schema migration breakage for existing stores.
  - Mitigation: additive schema only, guarded migration tests on legacy fixture.
- Risk: invalid relation payloads creating orphan/invalid edges.
  - Mitigation: strict validation + fail-closed writes.
