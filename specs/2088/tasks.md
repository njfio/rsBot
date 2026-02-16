# Tasks #2088

Status: Implemented
Spec: specs/2088/spec.md
Plan: specs/2088/plan.md

## Ordered Tasks

- T1 (RED): carry forward RED evidence from subtask `#2089` stale-exemption
  regression failure.
- T2 (GREEN): consume merged stale-exemption policy implementation from PR
  `#2090`.
- T3 (VERIFY): run
  `scripts/dev/test-oversized-file-policy.sh`,
  `scripts/dev/test-oversized-file-guardrail-contract.sh`,
  `python3 .github/scripts/test_oversized_file_guard.py`,
  and direct oversized guard command.
- T4 (CLOSE): set `specs/2088/*` Implemented and close task issue `#2088`.
