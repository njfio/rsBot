# Plan #2035

Status: Implemented
Spec: specs/2035/spec.md

## Approach

Reuse the existing dependency-drift checker as the single validation entrypoint,
extend it with namespace-prefix checks, and keep hierarchy validation intact.
Update policy + fixtures + docs in one change set.

## Affected Modules

- `tasks/policies/issue-hierarchy-drift-rules.json`
- `scripts/dev/dependency-drift-check.sh`
- `scripts/dev/test-dependency-drift-check.sh`
- `.github/scripts/test_issue_hierarchy_drift_rules.py`
- `docs/guides/issue-hierarchy-drift-rules.md`

## Risks and Mitigations

- Risk: Breaking existing hierarchy detection with new namespace handling.
  - Mitigation: Preserve legacy label compatibility and add fixture coverage.
- Risk: Drift policy/docs diverge.
  - Mitigation: Add policy-contract assertions in existing Python tests.

## Interfaces and Contracts

- Policy contract: required label prefixes and hierarchy rules in JSON.
- Runtime contract: dependency-drift checker condition IDs and severity output.
- Test contract: fixture-based script tests + policy-doc consistency tests.

## ADR References

- Not required.
