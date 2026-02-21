# Plan: Issue #3176 - tau-gaps report stale-claim resynchronization

## Approach
1. Capture current evidence (test counts, doc presence, key-rotation command evidence).
2. Update script expectations first (RED) so stale report fails.
3. Refresh report rows to satisfy new conformance checks (GREEN).
4. Re-run conformance script for pass and finalize issue/PR evidence.

## Affected Modules
- `tasks/tau-gaps-issues-improvements.md`
- `scripts/dev/test-tau-gaps-issues-improvements.sh`
- `specs/milestones/m221/index.md`
- `specs/3176/spec.md`
- `specs/3176/plan.md`
- `specs/3176/tasks.md`

## Risks & Mitigations
- Risk: report drift on future commits.
  - Mitigation: keep script checks focused on stable closure statements rather than brittle exact HEAD hash checks.
- Risk: overfitting script to wording.
  - Mitigation: assert deterministic fragments and stale exclusions only.

## Interfaces / Contracts
- Report closure markers in `tasks/tau-gaps-issues-improvements.md`.
- Conformance gate in `scripts/dev/test-tau-gaps-issues-improvements.sh`.

## ADR
No ADR required (documentation accuracy correction).
