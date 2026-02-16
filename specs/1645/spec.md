# Issue 1645 Spec

Status: Implemented

Issue: `#1645`  
Milestone: `#21`  
Parent: `#1618`

## Problem Statement

Safety smoke scenario and CI-light wiring exist from subtask work, but parent
task `#1645` remains open without issue-specific spec artifacts and parent-level
conformance evidence linking demo wrapper markers, CI manifest/workflow checks,
and operator troubleshooting documentation.

## Scope

In scope:

- add `specs/1645/{spec,plan,tasks}.md`
- add issue harness validating safety live-run contract markers across demo
  scripts, CI manifest/workflow, and docs
- document safety-smoke scenario in demo index guide (purpose, markers,
  troubleshooting)
- run targeted demo/contract tests and scoped quality checks

Out of scope:

- changing core safety enforcement logic
- introducing new CI workflows
- unrelated demo suite refactors

## Acceptance Criteria

AC-1 (demo safety smoke contract):
Given `scripts/demo/safety-smoke.sh` and demo index mappings,
when checked,
then deterministic fail-closed marker behavior is present for safety smoke.

AC-2 (CI smoke contract):
Given `.github/demo-smoke-manifest.json` and `.github/workflows/ci.yml`,
when checked,
then safety smoke command and validation steps are wired into CI-light smoke.

AC-3 (operator docs/troubleshooting):
Given `docs/guides/demo-index.md`,
when reviewed,
then safety-smoke scenario purpose, expected markers, and troubleshooting notes
are explicit.

AC-4 (verification):
Given issue-scope checks,
when run,
then harness + demo safety smoke tests + roadmap/fmt/clippy checks pass.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given safety smoke wrapper/index files, when checked by harness, then deterministic fail-closed marker tokens are present. |
| C-02 | AC-2 | CI verification | Given smoke manifest/workflow, when checked by harness, then safety smoke command/step wiring is present. |
| C-03 | AC-3 | Functional | Given demo index guide, when checked by harness, then safety-smoke docs section includes purpose/markers/troubleshooting content. |
| C-04 | AC-4 | Integration | Given issue commands, when run, then harness + `scripts/demo/test-safety-smoke.sh` + scoped checks pass. |

## Success Metrics

- `#1645` has complete spec-driven closure artifacts
- safety smoke live-run contract is explicit across wrapper/index/CI/docs
- failure output remains actionable for operators and CI maintainers
