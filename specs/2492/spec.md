# Spec #2492 - implement G9 phase-1 chunked ingestion + resume checkpoints

Status: Accepted

## Problem Statement
Bulk ingestion must process supported files safely and resumably. Today Tau has no ingestion worker path, so imports are manual and non-resilient.

## Acceptance Criteria
### AC-1 One-shot ingestion writes deterministic chunk memories
Given supported files in ingest directory, when `ingest_directory_once` runs, then each line-bounded chunk is persisted as a memory entry with deterministic source-event checkpoint key.

### AC-2 Reruns skip already-ingested chunks
Given chunks previously ingested, when ingestion reruns, then previously checkpointed chunks are skipped and no duplicate chunk entries are written.

### AC-3 File deletion is gated on full success
Given all chunks in a source file ingest successfully, when run completes, then source file is deleted; if any chunk fails, file remains for retry.

### AC-4 Unsupported files are skipped deterministically
Given files with unsupported extensions, when ingestion runs, then they are ignored and reported in ingestion diagnostics/counters.

## Scope
In scope:
- `tau-memory` ingestion API and tests.
- Supported extensions: txt, md, json, jsonl, csv, tsv, log, xml, yaml, toml.

Out of scope:
- Continuous watcher execution.
- LLM transform/summarization of chunks.

## Conformance Cases
- C-01 (AC-1, functional): `spec_2492_c01_ingestion_writes_deterministic_chunk_memories_for_supported_files`
- C-02 (AC-2, integration): `integration_spec_2492_c02_ingestion_rerun_skips_existing_chunk_checkpoints`
- C-03 (AC-3, regression): `regression_spec_2492_c03_ingestion_deletes_only_after_full_file_success`
- C-04 (AC-4, regression): `regression_spec_2492_c04_ingestion_skips_unsupported_extensions_with_counters`

## Success Metrics
- C-01..C-04 pass.
- No duplicate chunk checkpoints across reruns.
