# Issue 1629 Spec

Status: Implemented

Issue: `#1629`  
Milestone: `#21`  
Parent: `#1611`

## Problem Statement

`tau-memory` previously exposed a scaffolded postgres backend path. Runtime behavior has been shifted to explicit supported backends (`sqlite`, `jsonl`, `auto`), but issue `#1629` remains open without a dedicated conformance contract proving postgres is treated as unsupported and that operator docs are explicit.

## Scope

In scope:

- codify the postgres-backend disposition contract for runtime memory backend selection
- add tests-first conformance harness for unsupported backend fallback behavior
- align memory runbook wording to explicitly document unsupported postgres backend handling
- run scoped verification for memory backend behavior and docs guardrails

Out of scope:

- implementing a postgres backend
- changing memory data formats or adding dependencies
- changing public CLI flags or wire protocols

## Acceptance Criteria

AC-1 (unsupported backend fail-safe):
Given `TAU_MEMORY_BACKEND=postgres`,
when `FileMemoryStore` initializes,
then runtime selects a supported backend and sets reason code `memory_storage_backend_env_invalid_fallback`.

AC-2 (supported backend matrix):
Given memory backend resolution logic,
when inspected for accepted env values,
then only `auto`, `sqlite`, and `jsonl` are accepted and non-supported values are routed to fallback.

AC-3 (operator docs alignment):
Given `docs/guides/memory-ops.md`,
when reviewed,
then it explicitly states postgres backend is unsupported and falls back to inferred backend with reason code.

AC-4 (scoped verification):
Given the new conformance harness and existing regression test,
when run,
then both pass without introducing new warnings.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Regression | Given `TAU_MEMORY_BACKEND=postgres`, when running `regression_memory_store_treats_postgres_env_backend_as_invalid_and_falls_back`, then backend resolves to supported fallback with invalid-fallback reason code. |
| C-02 | AC-2 | Functional | Given backend resolver source, when checked by conformance harness, then accepted env values are limited to `auto/jsonl/sqlite` and invalid values use fallback path. |
| C-03 | AC-3 | Functional | Given memory ops runbook, when checked by conformance harness, then explicit unsupported-postgres note is present. |
| C-04 | AC-4 | Integration | Given issue-scope verification commands, when executed, then harness and targeted tau-memory regression test pass. |

## Success Metrics

- no production-facing postgres scaffold path remains documented as supported
- backend disposition contract is reproducible via deterministic harness + regression test
