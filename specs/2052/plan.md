# Plan #2052

Status: Implemented
Spec: specs/2052/spec.md

## Approach

Use existing CI entrypoints (`test-dependency-drift-check.sh` and Python
contract tests) as enforcement gates; update docs/policy/tests in lockstep.

## Affected Modules

- `scripts/dev/test-dependency-drift-check.sh`
- `.github/scripts/test_issue_hierarchy_drift_rules.py`
- `docs/guides/issue-hierarchy-drift-rules.md`

## Risks and Mitigations

- Risk: docs drift from policy condition IDs.
  - Mitigation: keep condition-ID assertion test active and updated.

## Interfaces and Contracts

- CI/local contract = script tests + Python policy contract tests.

## ADR References

- Not required.
