# Issue 1697 Spec

Status: Implemented

Issue: `#1697`  
Milestone: `#24`  
Parent: `#1662`

## Problem Statement

M24 requires reproducible benchmark fixtures that represent realistic reasoning
and tool-use workloads. Current coverage relies on toy vectors and does not
provide a stable fixture suite with documented scoring rubrics and deterministic
seeding.

## Scope

In scope:

- add benchmark fixture family files under
  `crates/tau-coding-agent/testdata/rl-benchmark-fixtures/`
- include reasoning and tool-use fixture families with deterministic seeds
- define and validate per-case scoring rubric contracts
- add `tau-trainer` fixture loader/validator APIs with conformance tests

Out of scope:

- live benchmark protocol publication (`#1698`)
- significance statistics engine work (`#1674`, `#1709`)
- rollout execution wiring into the runtime training loop

## Acceptance Criteria

AC-1 (fixture family coverage):
Given benchmark fixture files,
when the trainer fixture loader reads them,
then both reasoning and tool-use case families are present with deterministic
seeds and stable case IDs.

AC-2 (scoring rubric contract):
Given benchmark fixture scoring metadata,
when fixture validation runs,
then each rubric is finite, non-negative, and normalized to a deterministic
weight sum.

AC-3 (invalid fixture rejection):
Given malformed fixture input (duplicate case IDs, invalid weights, missing
required fields),
when the loader validates it,
then it fails with deterministic, actionable errors.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given checked-in reasoning and tool-use fixture files, when loaded, then family diversity and deterministic seeds/case IDs are verified. |
| C-02 | AC-2 | Conformance | Given valid fixture rubrics, when validated, then rubric dimensions are finite/non-negative and normalized. |
| C-03 | AC-3 | Regression | Given malformed fixture JSON, when loaded, then duplicate IDs, invalid weights, and missing fields are rejected with stable error contracts. |

## Success Metrics

- fixture suite contains reproducible reasoning and tool-use families
- rubric contract validation is enforced by deterministic tests
- malformed fixture cases are rejected with explicit diagnostics
