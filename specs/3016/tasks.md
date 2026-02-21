# Tasks: Issue #3016 - Contributor and security policy docs

## Ordered Tasks
1. [x] T1 (RED): add doc conformance script and capture failing output with missing root docs.
2. [x] T2 (GREEN): add `CONTRIBUTING.md` and `SECURITY.md` with required sections.
3. [x] T3 (REGRESSION): rerun conformance script to verify required markers.
4. [x] T4 (VERIFY): run `cargo fmt --check` and `cargo check -q`.

## Tier Mapping
- Unit: shell assertion helpers in doc conformance script
- Property: N/A (no algorithmic change)
- Contract/DbC: N/A (no API contracts changed)
- Snapshot: N/A (no snapshot tests)
- Functional: doc conformance script
- Conformance: C-01, C-02, C-03, C-04
- Integration: N/A (docs/script only)
- Fuzz: N/A (no parser fuzz surface changes)
- Mutation: N/A (docs/script only)
- Regression: conformance rerun after docs are added
- Performance: N/A (no runtime perf changes)
