# Spec #2497 - Implement G9 phase-2 ingestion worker and SHA-256 checkpoint tracking

Status: Implemented

## Problem Statement
`tau-memory` phase-1 ingestion currently uses non-SHA checkpoint material and a single entrypoint. We need a phase-2 implementation that introduces a worker-oriented API and durable SHA-256 checkpoint progress while preserving deterministic behavior.

## Acceptance Criteria
### AC-1 Worker entrypoint executes deterministic ingestion
Given an ingest directory and `MemoryIngestionOptions`, when worker entrypoint ingestion runs, then it delegates to the ingestion pipeline and returns deterministic counters/diagnostics equivalent to one-shot execution.

### AC-2 Checkpoint keys use SHA-256 digest material
Given chunk checkpoint material (`path|chunk_index|chunk_text`), when checkpoint keys are generated, then source-event keys include a lowercase 64-char SHA-256 digest.

### AC-3 Durable checkpoints prevent duplicates across reruns
Given successful prior chunk ingestion, when rerun occurs, then durable checkpoint state causes existing chunks to be skipped and no duplicate records are written.

### AC-4 Failure handling remains retry-safe
Given any chunk write failure in a file, when ingestion run completes, then the file is retained for retry and diagnostics include failure context.

## Scope
In scope:
- `tau-memory` ingestion worker entrypoint.
- SHA-256 checkpoint key generation and durable SQLite checkpoint persistence.
- Conformance/regression tests for rerun and failure semantics.

Out of scope:
- Filesystem watcher loops.
- New external APIs.

## Conformance Cases
- C-01 (AC-1, functional): `spec_2497_c01_worker_entrypoint_executes_ingestion_and_returns_counters`
- C-02 (AC-2, unit): `spec_2497_c02_checkpoint_key_uses_sha256_hex_digest`
- C-03 (AC-3, integration): `integration_spec_2497_c03_rerun_skips_chunks_from_durable_checkpoints`
- C-04 (AC-4, regression): `regression_spec_2497_c04_chunk_write_failure_keeps_source_file_for_retry`

## Success Metrics
- C-01..C-04 pass.
- Reruns produce zero duplicate chunk records.
