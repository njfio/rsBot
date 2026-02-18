# Spec #2503 - Implement G9 phase-3 watch polling and LLM chunk memory_save ingestion

Status: Implemented

## Problem Statement
The final unchecked G9 items require `tau-memory` to (1) watch the ingest directory via polling and (2) process each chunk through an LLM memory-save tool-call path.

## Acceptance Criteria
### AC-1 Watch poll mode short-circuits unchanged directories
Given a persistent polling state, when ingest directory contents/metadata are unchanged between polls, then ingestion is skipped with an explicit no-change diagnostic.

### AC-2 Watch poll mode ingests on change
Given new or modified supported files, when watch poll runs, then ingestion executes and updates polling state.

### AC-3 LLM mode processes chunk via memory_write tool-calls
Given OpenAI-compatible LLM options and chunk text, when worker ingestion runs in LLM mode, then chunk memory entries are produced from `memory_write` tool-call arguments.

### AC-4 LLM reruns remain checkpoint idempotent
Given successful LLM chunk ingestion, when rerun executes with unchanged files, then chunks are skipped via durable SHA-256 checkpoint keys.

### AC-5 Failure handling remains retry-safe
Given LLM extraction failure or malformed tool-call payload, when ingestion completes, then file is retained for retry and diagnostics include the failure reason.

## Scope
In scope:
- `FileMemoryStore` polling watcher APIs.
- `MemoryIngestionLlmOptions` and LLM extraction path.
- Conformance/regression coverage for watch + LLM modes.

Out of scope:
- notify-daemon implementation.
- Gateway-level ingestion APIs.

## Conformance Cases
- C-01 (AC-1, functional): `spec_2503_c01_watch_poll_skips_when_directory_unchanged`
- C-02 (AC-2, functional): `spec_2503_c02_watch_poll_processes_on_directory_change`
- C-03 (AC-3, integration): `integration_spec_2503_c03_llm_chunk_processing_uses_memory_write_tool_calls`
- C-04 (AC-4, integration): `integration_spec_2503_c04_llm_rerun_skips_durable_chunk_checkpoints`
- C-05 (AC-5, regression): `regression_spec_2503_c05_llm_parse_failure_keeps_source_file_for_retry`

## Success Metrics
- C-01..C-05 pass.
- G9 checklist items for watcher and LLM chunk processing are complete.
