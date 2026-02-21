# Plan: Issue #3136 - readme capability sync

## Approach
1. Capture RED evidence by asserting missing target phrases in current README.
2. Patch README capability/status sections with minimal docs-only edits.
3. Verify conformance via scoped grep checks.

## Affected Modules
- `README.md`
- `specs/milestones/m211/index.md`
- `specs/3136/spec.md`
- `specs/3136/plan.md`
- `specs/3136/tasks.md`

## Risks & Mitigations
- Risk: overstating dashboard maturity.
  - Mitigation: keep boundary statement that not all PRD surfaces are full live-mutation UX yet.
- Risk: docs drift with future slices.
  - Mitigation: keep wording concrete but not overly granular.

## Interfaces / Contracts
- Documentation contract only (no runtime/API changes).

## ADR
No ADR required.
