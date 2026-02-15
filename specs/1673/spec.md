# Issue 1673 Spec

Status: Implemented

Issue: `#1673`  
Milestone: `#24`  
Parent: `#1662`

## Problem Statement

M24 requires deterministic benchmark execution for baseline and trained policy
evaluation. Fixture families now exist, but `tau-trainer` lacks a benchmark
driver that executes suites with deterministic scoring and validates
repeatability across runs.

## Scope

In scope:

- add benchmark driver APIs in `tau-trainer` to execute fixture suites
- add deterministic scoring integration contract via scorer trait
- add repeatability evaluation across benchmark runs with tolerance thresholds
- add conformance tests for deterministic behavior and failure-path fixtures

Out of scope:

- live-run protocol/report publication (`#1698`, `#1709`)
- policy optimization runtime orchestration
- dashboard visualization

## Acceptance Criteria

AC-1 (deterministic benchmark driver):
Given a seeded benchmark fixture suite and deterministic scorer,
when benchmark runs execute repeatedly,
then observations and aggregate metrics are deterministic.

AC-2 (repeatability validation):
Given two or more benchmark run reports,
when repeatability is evaluated with a tolerance,
then per-case variance and overall within-band status are reported.

AC-3 (happy + failure path fixture coverage):
Given valid and malformed fixture inputs,
when benchmark setup loads fixtures,
then valid fixtures execute and malformed fixtures fail deterministically.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given deterministic scorer and seeded fixture suite, when run twice, then case scores and aggregate mean are identical. |
| C-02 | AC-2 | Unit | Given run reports with bounded/unbounded per-case deltas, when repeatability check runs, then within-band status and max delta match expected outcomes. |
| C-03 | AC-3 | Regression | Given malformed fixture files, when loader is used in benchmark setup, then deterministic validation errors are returned. |

## Success Metrics

- deterministic benchmark driver available for fixture suites
- repeatability report can gate variance bands for benchmark claims
- happy/failure path fixture handling is validated by tests
