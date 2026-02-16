# M25 Latency Budget Gate

Generated: `2026-02-16T15:10:00Z`
Repository: `njfio/Tau`
Policy: `tasks/policies/m25-latency-budget-policy.json`
Report: `tasks/reports/m25-fast-lane-loop-comparison.json`

## Summary

| Status | Violations |
|---|---:|
| pass | 0 |

## Report Metrics

| Metric | Value |
|---|---:|
| baseline_median_ms | 1006 |
| fast_lane_median_ms | 995 |
| improvement_percent | 1.09 |

## Checks

| Metric | Result | Threshold | Observed | Remediation |
|---|---|---|---:|---|
| fast_lane_median_ms | pass | <= 1050 | 995 | Trim wrapper scope or improve cache hit rate before widening loop coverage. |
| improvement_percent | pass | >= 0.5 | 1.09 | Revisit wrapper set and remove slow commands from the fast lane. |
| regression_percent | pass | <= 0.0 | 0.0 | Investigate newly regressed wrapper and update baseline only after root-cause analysis. |
