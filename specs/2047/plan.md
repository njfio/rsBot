# Plan #2047

Status: Implemented
Spec: specs/2047/spec.md

## Approach

1. Reuse merged `#2070` implementation (`#2082`) as the source of truth for
   workflow/shared-key/parallel-runner behavior.
2. Convert this parent task from placeholder drafts to implemented roll-up
   artifacts (spec/plan/tasks) with explicit AC -> conformance mapping.
3. Re-run task-scoped contract and integration suites to prove no regression in
   helper validation behavior.
4. Close `#2047` with status/phase log updates and conformance evidence.

## Affected Modules

- `specs/2047/spec.md`
- `specs/2047/plan.md`
- `specs/2047/tasks.md`
- `.github/scripts/test_ci_cache_parallel_contract.py`
- `.github/scripts/test_ci_helper_parallel_runner.py`
- `scripts/dev/test-ci-cache-parallel-tuning-report.sh`
- `tasks/reports/m25-ci-cache-parallel-tuning.json`
- `tasks/reports/m25-ci-cache-parallel-tuning.md`
- `.github/workflows/ci.yml`

## Risks and Mitigations

- Risk: Parent closure claims drift from merged subtask behavior.
  - Mitigation: conformance references point directly to checked-in workflow,
    tests, and report artifacts from merged PR `#2082`.
- Risk: Helper parallel runner could hide flaky failures.
  - Mitigation: rerun full helper discovery suite and preserve fail-closed
    regression cases in runner/report tests.

## Interfaces and Contracts

- Workflow contract:
  `.github/scripts/test_ci_cache_parallel_contract.py`
- Helper runner contract:
  `.github/scripts/test_ci_helper_parallel_runner.py`
- Timing report functional/regression suite:
  `scripts/dev/test-ci-cache-parallel-tuning-report.sh`

## ADR References

- Not required.
