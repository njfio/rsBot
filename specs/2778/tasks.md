# Tasks: Issue #2778 - G23 Fly.io CI pipeline validation (optional)

## Ordered Tasks
1. [x] T1 (RED): capture failing conformance check showing CI lacks Fly validation command.
2. [x] T2 (GREEN): add Fly scope detection and validation step(s) in `.github/workflows/ci.yml`.
3. [x] T3 (VERIFY): run workflow YAML parse/conformance commands and impacted test checks.
4. [x] T4 (DOC): update G23 roadmap row with `#2778` evidence and set spec implemented.

## Tier Mapping
- Unit: N/A (workflow config change)
- Property: N/A
- Contract/DbC: N/A
- Snapshot: N/A
- Functional: workflow command/path conformance checks
- Conformance: C-01..C-04
- Integration: CI workflow parse/invocation readiness
- Fuzz: N/A
- Mutation: N/A
- Regression: existing CI workflow behavior preserved
- Performance: N/A
