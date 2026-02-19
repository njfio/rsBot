# Tasks: Issue #2618 - Stage multi-process runtime architecture contracts (G1)

## Ordered Tasks
1. T1 (RED): add failing tests for process-type defaults and process-manager lifecycle supervision.
2. T2 (GREEN): implement `process_types` module and public API exports.
3. T3 (GREEN): implement supervisor lifecycle tracking (`ProcessManager`) with deterministic snapshots.
4. T4 (DOC): add ADR for staged multi-process migration boundary.
5. T5 (VERIFY): run scoped fmt/clippy/tests and map AC/C evidence.
6. T6 (CLOSE): update issue process log and open PR with tier matrix + TDD evidence.

## Tier Mapping
- Unit: C-01, C-02
- Property: N/A (no randomized invariant harness in this staged contract slice)
- Contract/DbC: N/A (no new contracts annotations)
- Snapshot: N/A (behavioral assertions over structured snapshot values)
- Functional: C-02 (supervisor state transitions)
- Conformance: C-01..C-05
- Integration: C-03 (existing single-loop branch flow unaffected)
- Fuzz: N/A (no new untrusted parser surface)
- Mutation: N/A (non-critical-path staging APIs)
- Regression: C-03
- Performance: N/A (no runtime hotspot/SLO changes)
