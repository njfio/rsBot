# Tasks: Issue #3036 - Contributor and Security policy docs hardening

## Ordered Tasks
1. [x] T1 (RED): extend docs conformance assertions for contributor/security links/sections and capture failing run.
2. [x] T2 (GREEN): update CONTRIBUTING/SECURITY/README content to satisfy conformance expectations.
3. [x] T3 (REGRESSION): rerun docs conformance script and targeted doc checks.
4. [x] T4 (VERIFY): run `cargo fmt --check` and `cargo check -q`.

## Tier Mapping
- Unit: N/A (docs-only change)
- Property: N/A (no randomized invariants)
- Contract/DbC: N/A (no contracts crate changes)
- Snapshot: N/A (no snapshot tests)
- Functional: docs conformance script asserts contract behavior
- Conformance: C-01..C-05
- Integration: N/A (no cross-service transport change)
- Fuzz: N/A (no untrusted parser/runtime surface)
- Mutation: N/A (docs-only non-critical slice)
- Regression: docs conformance rerun + baseline checks
- Performance: N/A (no runtime perf path changes)
