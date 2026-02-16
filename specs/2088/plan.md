# Plan #2088

Status: Implemented
Spec: specs/2088/spec.md

## Approach

1. Use merged implementation from subtask `#2089` as the source of truth.
2. Add task-level spec/plan/tasks artifacts with AC -> conformance mapping.
3. Re-run policy and guardrail validation suites for closure evidence.
4. Close task with status updates and parent-story handoff.

## Affected Modules

- `specs/2088/spec.md`
- `specs/2088/plan.md`
- `specs/2088/tasks.md`
- `scripts/dev/test-oversized-file-policy.sh`
- `scripts/dev/test-oversized-file-guardrail-contract.sh`
- `.github/scripts/test_oversized_file_guard.py`
- `tasks/policies/oversized-file-exemptions.json`

## Risks and Mitigations

- Risk: task closure may drift from merged subtask evidence.
  - Mitigation: re-run mapped suites on latest `master`.
- Risk: stale regressions not explicitly linked at task level.
  - Mitigation: include stale-check conformance mapping in spec and PR.

## Interfaces and Contracts

- Shell suites:
  `scripts/dev/test-oversized-file-policy.sh`,
  `scripts/dev/test-oversized-file-guardrail-contract.sh`
- Python suite:
  `python3 .github/scripts/test_oversized_file_guard.py`
- Direct guard command:
  `python3 .github/scripts/oversized_file_guard.py --repo-root . --default-threshold 4000 --exemptions-file tasks/policies/oversized-file-exemptions.json --policy-guide docs/guides/oversized-file-policy.md --json-output-file <path>`

## ADR References

- Not required.
