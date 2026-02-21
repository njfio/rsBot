# Tasks: Issue #3024 - Gateway API route inventory drift guard

## Ordered Tasks
1. [x] T1 (RED): add conformance test script for inventory/drift contract and capture failing output.
2. [x] T2 (GREEN): implement inventory script with deterministic JSON/Markdown outputs and drift enforcement.
3. [x] T3 (GREEN): update API reference command contract + docs conformance checks.
4. [x] T4 (REGRESSION): run new and existing docs conformance tests + generate report artifacts.
5. [x] T5 (VERIFY): run `cargo fmt --check` and `cargo check -q`.

## Tier Mapping
- Unit: shell assertions for script behavior
- Property: N/A (no randomized invariant surface)
- Contract/DbC: N/A (no Rust API contract changes)
- Snapshot: N/A (no snapshot harness)
- Functional: route-inventory script and docs conformance checks
- Conformance: C-01, C-02, C-03, C-04
- Integration: N/A (no cross-runtime behavior changes)
- Fuzz: N/A (no parser exposed to untrusted runtime input)
- Mutation: N/A (docs/script quality guard slice)
- Regression: rerun tests/checks after implementation
- Performance: N/A (no runtime performance path changes)
