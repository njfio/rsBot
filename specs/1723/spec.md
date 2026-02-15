# Issue 1723 Spec

Status: Implemented

Issue: `#1723`  
Milestone: `#23`  
Parent: `#1625`

## Problem Statement

Doc density failures currently surface only crate-level issues. PR authors lack
file-level hints pinpointing where undocumented public items caused regressions.

## Scope

In scope:

- add annotation script that maps failed crates to changed files
- emit GitHub workflow annotations with file/line hints for undocumented public items
- integrate script into CI after doc density analysis

Out of scope:

- changing density scoring rules
- adding external annotation services

## Acceptance Criteria

AC-1 (changed-file mapping):
Given failed crate thresholds and PR diff,
when annotation script runs,
then impacted files are mapped to failed crates.

AC-2 (file-level hints):
Given changed files in failed crates,
when script scans items,
then GitHub warning annotations include file and line hints.

AC-3 (CI integration):
Given doc-density check step,
when workflow runs,
then annotation script runs and posts hints without masking primary failure status.

AC-4 (regression safety):
Given script contract tests,
when run,
then annotation formatting and mapping behavior are verified.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given failed crates and changed-file list, when script runs, then impacted files are detected. |
| C-02 | AC-2 | Conformance | Given changed file with undocumented public item, when script runs, then `::warning file=...,line=...` output appears. |
| C-03 | AC-3 | Integration | Given CI job with rust scope, when doc density step runs, then annotation step executes with `always()` behavior. |
| C-04 | AC-4 | Regression | Given script tests, when executed, then annotation output contracts pass. |

## Success Metrics

- PRs receive actionable file-level doc-density hints
- signal quality improves for doc regression remediation
