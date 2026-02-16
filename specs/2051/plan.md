# Plan #2051

Status: Implemented
Spec: specs/2051/spec.md

## Approach

Add prefix validation to dependency drift checker and preserve legacy hierarchy
compatibility through normalized type handling.

## Affected Modules

- `scripts/dev/dependency-drift-check.sh`
- `scripts/dev/test-dependency-drift-check.sh`
- `tasks/policies/issue-hierarchy-drift-rules.json`

## Risks and Mitigations

- Risk: False positives on parent compatibility with `type:*` labels.
  - Mitigation: Normalize parent labels to include unprefixed type aliases.

## Interfaces and Contracts

- Condition output contract in drift checker report JSON and logs.

## ADR References

- Not required.
