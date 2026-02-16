# Plan #2089

Status: Implemented
Spec: specs/2089/spec.md

## Approach

1. Add RED regression coverage for stale exemptions in policy shell tests.
2. Implement active-size eligibility validation in
   `scripts/dev/oversized-file-policy.sh` by checking current line counts for
   each exemption path.
3. Remove stale entries from `tasks/policies/oversized-file-exemptions.json`.
4. Run policy and guardrail suites to verify fail-closed behavior and no drift.

## Affected Modules

- `scripts/dev/oversized-file-policy.sh`
- `scripts/dev/test-oversized-file-policy.sh`
- `tasks/policies/oversized-file-exemptions.json`
- `scripts/dev/test-oversized-file-guardrail-contract.sh`
- `specs/2089/spec.md`
- `specs/2089/plan.md`
- `specs/2089/tasks.md`
- `specs/milestones/m26/index.md`

## Risks and Mitigations

- Risk: line-count check could fail on missing paths.
  - Mitigation: emit explicit fail-closed error with path context.
- Risk: stale-exemption cleanup might hide needed temporary exemption.
  - Mitigation: guard script already reports oversized files without exemptions;
    verify with oversized guard command in the test loop.

## Interfaces and Contracts

- Policy validator:
  `scripts/dev/oversized-file-policy.sh --exemptions-json <path>`
- Validation suites:
  `scripts/dev/test-oversized-file-policy.sh`
  `scripts/dev/test-oversized-file-guardrail-contract.sh`
  `python3 .github/scripts/test_oversized_file_guard.py`

## ADR References

- Not required.
