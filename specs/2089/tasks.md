# Tasks #2089

Status: Implemented
Spec: specs/2089/spec.md
Plan: specs/2089/plan.md

## Ordered Tasks

- T1 (RED): add stale-exemption failing case in
  `scripts/dev/test-oversized-file-policy.sh`.
- T2 (GREEN): implement active-size eligibility checks in
  `scripts/dev/oversized-file-policy.sh` and clean stale exemption entries in
  `tasks/policies/oversized-file-exemptions.json`.
- T3 (VERIFY): run
  `scripts/dev/test-oversized-file-policy.sh`,
  `scripts/dev/test-oversized-file-guardrail-contract.sh`,
  `python3 .github/scripts/test_oversized_file_guard.py`,
  and the oversized-file guard command against repo policy paths.
- T4 (CLOSE): set `specs/2089/*` to Implemented and close issue `#2089` with PR
  evidence.
