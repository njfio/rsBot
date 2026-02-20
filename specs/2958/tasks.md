# Tasks: Issue #2958 - Operator deployment guide and live validation

## Ordered Tasks
1. [x] T1 (RED): confirm operator deployment guide path is missing and capture failing evidence.
2. [x] T2 (GREEN): author `docs/guides/operator-deployment-guide.md` with prerequisite/setup/launch/validation/rollback flow.
3. [x] T3 (GREEN): add `docs/README.md` index entry for the new guide.
4. [x] T4 (REGRESSION): execute documented live validation commands and confirm expected gate posture.
5. [x] T5 (VERIFY): run docs-focused checks and finalize issue/PR contract artifacts.

## Tier Mapping
- Unit: N/A (docs-only)
- Property: N/A (docs-only)
- Contract/DbC: N/A (docs-only)
- Snapshot: N/A (docs-only)
- Functional: documented command flow executes
- Conformance: C-01..C-04 evidence in PR
- Integration: operator readiness script + gateway status/cortex status checks
- Fuzz: N/A (no parser changes)
- Mutation: N/A (docs-only)
- Regression: docs quality CI + live command rerun
- Performance: N/A (docs-only)
