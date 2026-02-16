# Plan #2036

Status: Implemented
Spec: specs/2036/spec.md

## Approach

Use GitHub milestone metadata as source of truth, compare with local milestone
index paths, backfill missing containers, and publish coverage artifacts.

## Affected Modules

- `specs/milestones/m1` through `specs/milestones/m20` (backfilled)
- `tasks/reports/m25-milestone-spec-index-coverage.json`
- `tasks/reports/m25-milestone-spec-index-coverage.md`

## Risks and Mitigations

- Risk: Incomplete milestone query scope.
  - Mitigation: query `state=all` with `per_page=100`.

## Interfaces and Contracts

- Contract: each milestone maps to `specs/milestones/m<number>/index.md`.

## ADR References

- Not required.
