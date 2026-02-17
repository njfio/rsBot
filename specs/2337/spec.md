# Spec #2337

Status: Implemented
Milestone: specs/milestones/m54/index.md
Issue: https://github.com/njfio/Tau/issues/2337

## Problem Statement

Roadmap claim #8 remains partial because dashboard capability is perceived as
scaffolded. Current code serves dashboard health/widgets/actions/stream through
`tau-gateway`, but there is no single verification command plus architecture
decision record proving that consolidation direction.

## Scope

In scope:

- Add ADR defining dashboard consolidation on gateway runtime.
- Add `scripts/dev/verify-dashboard-consolidation.sh` that runs gateway dashboard
  integration coverage plus onboarding regression around removed contract runner.
- Update wave-2 roadmap claim #8 to `Resolved` with executable evidence.

Out of scope:

- Implementing a standalone dashboard frontend application.
- Removing crates/files from workspace.

## Acceptance Criteria

- AC-1: Given docs and architecture decisions, when reviewing ADR content, then
  dashboard runtime ownership is explicitly consolidated to `tau-gateway` with
  rationale and consequences.
- AC-2: Given local checkout, when running
  `scripts/dev/verify-dashboard-consolidation.sh`, then mapped dashboard tests
  execute deterministically and fail closed.
- AC-3: Given updated roadmap, when reviewing claim #8 row, then status is
  `Resolved` with evidence referencing the consolidation verifier.
- AC-4: Given current branch state, when running
  `scripts/dev/verify-dashboard-consolidation.sh`, then it exits `0`.

## Conformance Cases

- C-01 (AC-1, conformance): ADR file exists at
  `docs/architecture/adr-001-dashboard-consolidation.md` with Context/Decision/
  Consequences.
- C-02 (AC-2, integration): verifier script exists and executes mapped tests:
  gateway dashboard endpoints/actions/stream + onboarding regression.
- C-03 (AC-3, conformance): `tasks/resolution-roadmap.md` claim #8 is updated to
  `Resolved` with command-level evidence.
- C-04 (AC-4, functional): verifier script run exits successfully on branch.

## Success Metrics / Observable Signals

- Dashboard consolidation stance is explicit and reviewable.
- One command validates non-scaffold dashboard behavior through gateway.
- Wave-2 roadmap no longer marks claim #8 as partial.
