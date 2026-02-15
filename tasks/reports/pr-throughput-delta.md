# PR Throughput Delta Report

- Generated at: 2026-02-15T09:54:15Z
- Reporting interval: daily
- Baseline generated at: 2026-02-15T09:50:16Z
- Current generated at: 2026-02-15T09:54:15Z
- Baseline sample count: 60
- Current sample count: 60
- Reproduce:
  - `scripts/dev/pr-throughput-delta-report.sh --baseline-json tasks/reports/pr-throughput-baseline.json --reporting-interval daily --repo njfio/Tau --since-days 30 --limit 60 --output-md tasks/reports/pr-throughput-delta.md --output-json tasks/reports/pr-throughput-delta.json`

## Delta Summary

- Improved metrics: 1
- Regressed metrics: 2
- Flat metrics: 0
- Unknown metrics: 0

## Average Delta (lower is better)

| Metric | Baseline Avg | Current Avg | Delta | Delta % | Status |
| --- | ---: | ---: | ---: | ---: | --- |
| PR age (created -> merged) | 46.56m | 42.63m | -3.93m | -8.45% | improved |
| Review latency (created -> first review) | 5.15m | 5.19m | 2s | +0.71% | regressed |
| Merge interval (between merged PRs) | 11.42m | 11.56m | 8s | +1.22% | regressed |

## Notes Template

- Wins observed:
  - <capture the highest-confidence contributors to improvements>
- Regressions observed:
  - <capture the highest-impact regressions and likely causes>
- Next actions:
  1. <action 1>
  2. <action 2>
  3. <action 3>
