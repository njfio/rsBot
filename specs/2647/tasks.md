# Tasks: Issue #2647 - External coding-agent subprocess worker support in bridge runtime (G21 phase 3)

## Ordered Tasks
1. T1 (RED): add failing subprocess-focused conformance/regression tests for C-01..C-06 in `tau-runtime`.
2. T2 (GREEN): implement subprocess config + spawn/reuse + stdout/stderr capture + follow-up stdin forwarding.
3. T3 (GREEN): implement close/reap subprocess termination and process-exit lifecycle synchronization.
4. T4 (GREEN/REGRESSION): update gateway bridge error mapping/tests only as needed for compatibility.
5. T5 (VERIFY): run scoped fmt/clippy/targeted tests and collect evidence for AC/C mapping.
6. T6 (CLOSE): update roadmap checklist and issue/process logs, then prepare PR artifacts.

## Tier Mapping
- Unit: C-01
- Property: N/A (no randomized invariant surface added)
- Contract/DbC: N/A (no contracts macro surfaces changed)
- Snapshot: N/A (behavioral assertions on structured events)
- Functional: C-02, C-03
- Conformance: C-01..C-07
- Integration: C-04, C-05, C-06 (bridge runtime + process lifecycle composition)
- Fuzz: N/A (no untrusted parser changes)
- Mutation: N/A (non-critical-path runtime integration slice)
- Regression: C-05, C-06
- Performance: N/A (no SLO target change)
