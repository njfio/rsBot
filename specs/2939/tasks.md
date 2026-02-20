# Tasks: Issue #2939 - Deploy Agent wizard panel and conformance tests

## Ordered Tasks
1. [x] T1 (RED): add failing conformance tests for C-01..C-05 in `tau-dashboard-ui` render tests.
2. [x] T2 (GREEN): add route-aware render function and deploy panel markers for `/ops/deploy`.
3. [x] T3 (GREEN): keep baseline shell marker tests green via compatibility wrapper.
4. [x] T4 (REGRESSION): verify non-deploy routes omit deploy markers.
5. [x] T5 (VERIFY): run fmt, clippy, and scoped tests; update spec status to Implemented.

## Tier Mapping
- Unit: render helper coverage for deploy/non-deploy route behavior.
- Property: N/A (static marker contracts).
- Contract/DbC: N/A (no contracts macro in module).
- Snapshot: N/A (marker asserts are explicit).
- Functional: deploy route markers present.
- Conformance: C-01..C-05 mapped to `spec_c0x_*` tests.
- Integration: N/A (crate-local rendering contract slice).
- Fuzz: N/A (no parser/untrusted input path).
- Mutation: N/A (UI marker scaffolding task; low algorithmic branching).
- Regression: non-deploy routes exclude deploy panel markers.
- Performance: N/A (no runtime-critical path change).
