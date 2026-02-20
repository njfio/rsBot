# Tasks: Issue #2721 - Integrate ProcessManager/runtime profiles into live branch-worker execution

## Ordered Tasks
1. [x] T1 (RED): add failing conformance/regression tests for C-01..C-04 (lineage, worker profile enforcement, terminal states).
2. [x] T2 (GREEN): add process context/runtime profile helpers to `Agent` and expose snapshot accessors for verification.
3. [x] T3 (GREEN): refactor branch follow-up execution to supervised branch/worker tasks through `ProcessManager`.
4. [x] T4 (GREEN): add deterministic `process_delegation` payload metadata while preserving existing reason codes.
5. [x] T5 (REGRESSION): run/adjust existing `spec_2602` coverage to verify unchanged guardrails.
6. [x] T6 (VERIFY): run scoped fmt/clippy/targeted tests and capture C-06 evidence.
7. [x] T7 (DOC): update `tasks/spacebot-comparison.md` G1 checklist entries completed by this slice.

## Tier Mapping
- Unit: process context helpers/profile application behavior.
- Property: N/A (no parser/invariant randomization introduced).
- Contract/DbC: N/A (no contracts macro adoption in this slice).
- Snapshot: N/A (explicit behavior assertions are used).
- Functional: C-02, C-03.
- Conformance: C-01..C-06.
- Integration: C-01, C-03, C-04.
- Fuzz: N/A (no new untrusted parser boundary introduced).
- Mutation: N/A (non-critical feature slice; follow-up if designated critical path).
- Regression: C-04, C-05.
- Performance: N/A (no new hotspot SLA contract introduced).
