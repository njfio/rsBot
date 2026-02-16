# Plan #2071

Status: Reviewed
Spec: specs/2071/spec.md

## Approach

1. Add a latency-budget policy artifact under `tasks/policies/`.
2. Implement a deterministic gate script that:
   - reads comparison report + policy,
   - computes pass/fail status,
   - writes JSON + Markdown diagnostics artifacts.
3. Add shell and Python contract tests covering pass/fail paths and malformed
   input handling.
4. Document operator usage for local/CI gating execution.

## Affected Modules

- `tasks/policies/m25-latency-budget-policy.json`
- `scripts/dev/latency-budget-gate.sh`
- `scripts/dev/test-latency-budget-gate.sh`
- `.github/scripts/test_latency_budget_gate_contract.py`
- `tasks/reports/m25-latency-budget-gate.json`
- `tasks/reports/m25-latency-budget-gate.md`
- `docs/guides/latency-budget-gate.md`
- `specs/2071/spec.md`
- `specs/2071/plan.md`
- `specs/2071/tasks.md`

## Risks and Mitigations

- Risk: unstable timing noise causes false gate failures.
  - Mitigation: evaluate policy against median metrics and include tolerance
    fields in policy.
- Risk: diagnostics are too vague to act on.
  - Mitigation: enforce required remediation text in failing diagnostics.

## Interfaces and Contracts

- Gate script:
  `scripts/dev/latency-budget-gate.sh --report-json <path> --policy-json <path>`
- Test suites:
  `scripts/dev/test-latency-budget-gate.sh`
  `python3 .github/scripts/test_latency_budget_gate_contract.py`

## ADR References

- Not required.
