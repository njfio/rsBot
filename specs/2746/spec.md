# Spec: Issue #2746 - G18 dashboard architecture/stack decision ADR closure

Status: Accepted

## Problem Statement
G18 still lists architecture location and tech stack decisions as unresolved even though implementation is actively progressing in gateway-hosted shell slices. This creates planning ambiguity and weakens governance traceability.

## Acceptance Criteria

### AC-1 ADR documents dashboard architecture location decision
Given current gateway-hosted dashboard slices,
When ADR is published,
Then it explicitly states implementation location and rationale.

### AC-2 ADR documents selected stack and migration posture
Given current embedded shell and future roadmap,
When ADR is published,
Then it explicitly states the selected stack and incremental migration strategy.

### AC-3 G18 checklist decision rows are updated with evidence
Given ADR publication,
When `tasks/spacebot-comparison.md` is updated,
Then decision and tech stack rows are marked complete with issue/ADR evidence links.

### AC-4 Documentation coherence is preserved
Given updated ADR/checklist,
When docs review runs,
Then no contradictory dashboard architecture statements remain in touched files.

## Scope

### In Scope
- New ADR for dashboard architecture/stack decision.
- Checklist updates in `tasks/spacebot-comparison.md`.
- Spec/plan/tasks artifacts for traceability.

### Out of Scope
- New runtime/UI behavior changes.
- Dependency additions.
- CI/CD changes.

## Conformance Cases
- C-01 (docs): ADR exists at `docs/architecture/adr-006-dashboard-ui-stack.md` with Context/Decision/Consequences.
- C-02 (docs): checklist rows updated and linked to issue evidence.
- C-03 (docs regression): touched docs remain consistent with existing dashboard implementation path.
- C-04 (verify): `cargo fmt --check` and `cargo clippy -p tau-gateway -- -D warnings` remain green (no behavior regressions introduced).

## Success Metrics / Observable Signals
- G18 decision and stack rows no longer unresolved.
- Dashboard architecture choice is auditable via ADR.
- No code/runtime regressions from docs-only slice.
