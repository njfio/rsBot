# Tasks: Issue #2691 - PRD gateway tools inventory and stats endpoints

## Ordered Tasks
1. [x] T1 (RED): add failing integration/regression tests for C-01..C-06.
2. [x] T2 (GREEN): add tools routes and status discovery metadata.
3. [x] T3 (GREEN): implement tool inventory and telemetry stats aggregation handlers.
4. [x] T4 (REGRESSION): verify missing/malformed telemetry fallback and unauthorized fail-closed behavior.
5. [x] T5 (VERIFY): run scoped fmt/clippy/targeted tests and capture C-07 evidence.

## Tier Mapping
- Unit: endpoint helper behavior coverage.
- Property: N/A (no randomized invariant algorithm introduced).
- Contract/DbC: N/A (contracts macros not introduced in touched modules).
- Snapshot: N/A (explicit field assertions used).
- Functional: C-01, C-02.
- Conformance: C-01..C-07.
- Integration: C-01, C-02, C-06.
- Fuzz: N/A (no new parser/codec boundary requiring fuzz harness in this bounded slice).
- Mutation: N/A (bounded additive endpoint slice).
- Regression: C-03, C-04, C-05, C-06.
- Performance: N/A (no hotspot/perf budget contract changed).
