# M54 — Dashboard Consolidation Decision and Verification

Milestone: [GitHub milestone #54](https://github.com/njfio/Tau/milestone/54)

## Objective

Resolve the dashboard partial-gap claim by documenting the architecture decision
to consolidate dashboard runtime behavior in `tau-gateway` and validating that
behavior with an executable verification runner.

## Scope

- Add dashboard consolidation ADR under `docs/architecture/`.
- Add `scripts/dev/verify-dashboard-consolidation.sh`.
- Update roadmap claim #8 evidence/status using the new verifier.

## Out of Scope

- Building a separate standalone dashboard frontend runtime.
- Changing gateway API contracts beyond existing tested endpoints.

## Linked Hierarchy

- Epic: #2334
- Story: #2335
- Task: #2336
- Subtask: #2337
