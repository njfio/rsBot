# Plan #2069

Status: Reviewed
Spec: specs/2069/spec.md

## Approach

1. Add `scripts/dev/fast-lane-dev-loop.sh` with:
   - wrapper catalog (`list`),
   - command execution (`run <id>`),
   - benchmark report generation (`benchmark`).
2. Emit benchmark comparison artifacts in `tasks/reports/` by comparing wrapper
   measurements to `tasks/reports/m25-build-test-latency-baseline.json`.
3. Add shell + Python contract tests and publish usage guide.

## Affected Modules

- `scripts/dev/fast-lane-dev-loop.sh`
- `scripts/dev/test-fast-lane-dev-loop.sh`
- `.github/scripts/test_fast_lane_dev_loop_contract.py`
- `docs/guides/fast-lane-dev-loop.md`
- `tasks/reports/m25-fast-lane-loop-comparison.json`
- `tasks/reports/m25-fast-lane-loop-comparison.md`
- `specs/2069/spec.md`
- `specs/2069/plan.md`
- `specs/2069/tasks.md`

## Risks and Mitigations

- Risk: benchmark variance from warm caches may fluctuate.
  - Mitigation: use fixed wrapper set and explicit benchmark timestamp; compare
    medians, not single outliers.
- Risk: wrapper set drifts from documented usage.
  - Mitigation: `list` output contract is tested against required IDs.

## Interfaces and Contracts

- Wrapper script:
  `scripts/dev/fast-lane-dev-loop.sh list|run|benchmark ...`
- Shell tests:
  `scripts/dev/test-fast-lane-dev-loop.sh`
- Contract tests:
  `python3 .github/scripts/test_fast_lane_dev_loop_contract.py`

## ADR References

- Not required.
