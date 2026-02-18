# Spec #2496 - worker-oriented ingestion orchestration + SHA-256 checkpoints

Status: Implemented

## Problem Statement
`tau-memory` needs a worker-oriented ingestion flow that can be invoked deterministically while storing durable SHA-256 checkpoints for safe reruns after interruptions.

## Acceptance Criteria
### AC-1 Worker-oriented ingestion entrypoint exists
Given an ingest directory and options, when worker ingestion is invoked, then ingestion executes through a dedicated worker entrypoint and returns deterministic counters/diagnostics.

### AC-2 Checkpoint material uses SHA-256
Given a source path, chunk index, and chunk text, when checkpoint keys are computed, then the digest component is a deterministic lowercase SHA-256 hex string.

### AC-3 Durable checkpoint progress survives reruns
Given previously ingested chunks, when ingestion reruns, then checkpoints are loaded from durable storage and duplicate chunks are skipped.

### AC-4 Existing lifecycle guarantees are preserved
Given mixed ingest success and failures, when run completes, then successful files are deletable per option and failed files remain for retry with diagnostics.

## Scope
In scope:
- Worker-style entrypoint in `tau-memory`.
- SHA-256 chunk checkpoint computation and durable SQLite persistence.
- Compatibility with pre-existing deterministic rerun skip behavior.

Out of scope:
- Background file watching.
- New retrieval/ranking behavior.

## Conformance Cases
- C-01 (AC-1, functional): worker entrypoint runs one-shot ingestion and returns counters.
- C-02 (AC-2, unit): checkpoint hash material renders a stable SHA-256 digest.
- C-03 (AC-3, integration): rerun uses durable checkpoints to skip previously ingested chunks.
- C-04 (AC-4, regression): file lifecycle + diagnostics remain fail-safe.

## Success Metrics
- C-01..C-04 pass in scoped `tau-memory` tests.
- No duplicate chunk ingestion across reruns.
