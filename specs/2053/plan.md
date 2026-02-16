# Plan #2053

Status: Implemented
Spec: specs/2053/spec.md

## Approach

Enumerate milestones via GitHub API, compare against `specs/milestones/m<number>/index.md`,
and write deterministic JSON/markdown artifacts.

## Affected Modules

- `tasks/reports/m25-milestone-spec-index-coverage.json`
- `tasks/reports/m25-milestone-spec-index-coverage.md`

## Risks and Mitigations

- Risk: Query only open milestones.
  - Mitigation: enforce `state=all` in coverage generation command.

## Interfaces and Contracts

- Coverage artifact schema fields (`total_milestones`, `covered`, `missing`, `rows`).

## ADR References

- Not required.
