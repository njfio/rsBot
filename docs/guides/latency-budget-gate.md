# Latency Budget Gate (M25.4.4a)

This guide documents policy thresholds and gate checks for M25 latency-budget
enforcement.

Gate script:

- `scripts/dev/latency-budget-gate.sh`

Policy artifact:

- `tasks/policies/m25-latency-budget-policy.json`

Validation suites:

- `scripts/dev/test-latency-budget-gate.sh`
- `python3 .github/scripts/test_latency_budget_gate_contract.py`

Gate report artifacts:

- `tasks/reports/m25-latency-budget-gate.json`
- `tasks/reports/m25-latency-budget-gate.md`

## Run Gate Check

```bash
scripts/dev/latency-budget-gate.sh \
  --policy-json tasks/policies/m25-latency-budget-policy.json \
  --report-json tasks/reports/m25-fast-lane-loop-comparison.json \
  --output-json tasks/reports/m25-latency-budget-gate.json \
  --output-md tasks/reports/m25-latency-budget-gate.md
```

The command exits non-zero when policy violations exist and
`enforcement_mode=fail`.

## Policy Thresholds

- `max_fast_lane_median_ms`: upper bound for fast-lane median duration.
- `min_improvement_percent`: minimum required improvement vs baseline median.
- `max_regression_percent`: maximum tolerated regression percent (when
  improvement is negative).
- `remediation`: per-metric operator guidance included in gate diagnostics.
