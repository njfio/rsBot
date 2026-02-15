# Issue 1724 Spec

Status: Implemented

Issue: `#1724`  
Milestone: `#24`  
Parent: `#1658`

## Problem Statement

RL schema structs (`EpisodeTrajectory`, `AdvantageBatch`, `CheckpointRecord`)
must remain evolution-safe. Without explicit migration/version tests, schema
changes risk silent decode drift or ambiguous failure modes.

## Scope

In scope:

- add migration fixtures for legacy payloads that omit schema version
- add unknown-version failure tests for RL schema structs
- document migration guarantees directly in RL schema comments/tests

Out of scope:

- introducing schema version `v2`
- storage backend migration tooling beyond schema validation tests

## Acceptance Criteria

AC-1 (legacy fixture upgrade behavior):
Given legacy payloads without `schema_version`,
when deserialized,
then schema structs default to v1 and validate successfully.

AC-2 (unknown-version fail closed):
Given payloads with unsupported schema versions,
when validated,
then validation fails with deterministic `unsupported schema version` errors.

AC-3 (migration guarantees documented):
Given RL schema code/tests,
when reviewed,
then migration behavior (legacy default + unknown-version fail closed) is
explicitly documented.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Unit | Given legacy trajectory/advantage/checkpoint payloads, when deserialized, then version defaults to v1 and validate passes. |
| C-02 | AC-2 | Regression | Given unsupported schema versions, when validate runs, then deterministic unsupported-version errors are returned. |
| C-03 | AC-3 | Functional | Given code/tests, when scanned, then migration guarantees are explicitly asserted and described. |

## Success Metrics

- schema evolution behavior is test-enforced and deterministic for M24 data flow
