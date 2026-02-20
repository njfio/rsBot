# Tasks: Issue #2782 - PRD Phase 1A Leptos crate and /ops shell integration

## Ordered Tasks
1. [x] T1 (RED): capture failing checks for missing `tau-dashboard-ui` crate / `/ops` route contracts.
2. [x] T2 (GREEN): add `tau-dashboard-ui` crate and workspace/dependency wiring.
3. [x] T3 (GREEN): implement SSR shell render function with baseline markers and crate tests.
4. [x] T4 (GREEN): integrate `/ops` endpoint in gateway and add integration coverage.
5. [x] T5 (REGRESSION): run scoped gateway/dashboard shell regression tests.
6. [x] T6 (VERIFY): run fmt/clippy/tests and set spec implemented.

## Tier Mapping
- Unit: crate render marker assertions
- Property: N/A
- Contract/DbC: N/A
- Snapshot: N/A
- Functional: SSR render output contracts
- Conformance: C-01..C-04
- Integration: gateway `/ops` endpoint and existing `/dashboard` endpoint tests
- Fuzz: N/A
- Mutation: N/A (UI foundation scaffold)
- Regression: existing dashboard shell behavior
- Performance: N/A
