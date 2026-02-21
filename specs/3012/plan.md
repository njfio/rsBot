# Plan: Issue #3012 - Review #31 stale reference correction

## Approach
1. Add stale-name assertions to the existing Review #31 conformance script.
2. Run script first to capture RED on current stale references.
3. Correct Review #31 crate rows to valid current crate names and signals.
4. Re-run conformance + baseline checks for GREEN/regression evidence.

## Affected Paths
- `tasks/tau-gaps-issues-improvements.md`
- `scripts/dev/test-tau-gaps-issues-improvements.sh`
- `specs/milestones/m181/index.md`
- `specs/3012/spec.md`
- `specs/3012/plan.md`
- `specs/3012/tasks.md`

## Risks and Mitigations
- Risk: introducing further drift in large doc.
  - Mitigation: keep edit tightly scoped to stale rows + guard assertions.
- Risk: brittle conformance checks.
  - Mitigation: assert only stable stale-name absence and required core markers.

## ADR
Not required (docs + conformance script only).
