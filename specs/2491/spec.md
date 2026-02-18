# Spec #2491 - tau-memory ingestion worker foundation

Status: Implemented

## Problem Statement
`tau-memory` currently has no reusable ingestion worker API for batch files, leaving G9 unimplemented.

## Acceptance Criteria
### AC-1 Supported file discovery and chunking are deterministic
Given an ingest directory with supported files, when ingestion runs, then files are processed in deterministic order and chunked at line boundaries using configured chunk size.

### AC-2 Durable chunk checkpoints enable safe reruns
Given reruns with previously ingested chunks, when ingestion runs again, then already-checkpointed chunks are skipped and only new chunks are ingested.

### AC-3 Source file lifecycle is fail-safe
Given file ingestion succeeds for all chunks, when run completes, then source file is deleted; if any chunk fails, source file is retained.

## Scope
In scope:
- One-shot ingestion API in `tau-memory`.
- Checkpointing via deterministic chunk source-event keys backed by existing memory persistence.

Out of scope:
- Background watch loops.
- LLM summarization/transform step.

## Conformance Cases
- C-01 (AC-1, functional): supported files ingest into deterministic chunk records.
- C-02 (AC-2, integration): rerun skips existing chunks and ingests only missing chunks.
- C-03 (AC-3, regression): mixed success/failure retains file on failure, deletes on success.

## Success Metrics
- C-01..C-03 pass under `tau-memory` tests.
- No duplicate chunk ingestion across reruns.
