# Issue 1655 Spec

Status: Implemented

Issue: `#1655`  
Milestone: `#23`  
Parent: `#1625`

## Problem Statement

M23 requires sustained documentation growth, but current CI only enforces API
coverage thresholds. It does not enforce a ratcheting raw-marker floor or emit
clear per-crate regression diffs when marker totals fall.

## Scope

In scope:

- add CI-checkable ratchet policy for raw marker floor
- add script that validates current marker totals against ratchet floor
- emit per-crate regression diff artifact for operator review
- wire check into CI workflow and upload artifacts

Out of scope:

- changing marker counting semantics
- forcing threshold `>=3000` in this issue

## Acceptance Criteria

AC-1 (ratchet floor contract):
Given ratchet policy,
when current marker total is below configured floor,
then CI check fails with explicit threshold diagnostics.

AC-2 (regression diff output):
Given baseline/current marker artifacts,
when ratchet check runs,
then per-crate marker deltas are emitted in JSON/Markdown artifacts.

AC-3 (CI integration):
Given PR CI run with Rust changes,
when doc marker check executes,
then ratchet script runs and uploads artifact.

AC-4 (regression safety):
Given script/unit contract tests,
when executed,
then ratchet behavior and output schema are validated.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given floor > current, when script runs, then exit is non-zero with failure summary. |
| C-02 | AC-2 | Conformance | Given baseline/current artifacts, when script runs, then delta rows and totals are present. |
| C-03 | AC-3 | Integration | Given CI workflow, when Rust scope is true, then ratchet step and artifact upload execute. |
| C-04 | AC-4 | Regression | Given script test harness, when run, then pass/fail paths are validated. |

## Success Metrics

- CI enforces non-regressing raw marker floor
- regression diffs are visible as uploaded artifacts
