# Spec #2501 - phase-3 watcher polling and LLM chunk memory_save orchestration

Status: Accepted

## Problem Statement
`tau-memory` ingestion has deterministic one-shot execution, but it cannot watch an ingest directory over polling cycles or route chunk ingestion through an LLM tool-call extraction flow.

## Acceptance Criteria
### AC-1 Polling watcher can detect ingest directory changes
Given a persisted polling state and ingest directory, when watch poll runs, then unchanged polls short-circuit with diagnostics and changed polls execute ingestion.

### AC-2 LLM chunk extraction path is available
Given OpenAI-compatible LLM options, when ingestion runs in LLM mode, then each chunk is processed from `memory_write`-style tool-call output before persistence.

### AC-3 Existing checkpoint + lifecycle guarantees remain intact
Given reruns and mixed success/failure files, when phase-3 flow executes, then SHA-256 checkpoint dedupe and delete-on-success semantics are preserved.

## Scope
In scope:
- Polling state structure and watch poll APIs.
- LLM extraction request/parse flow for memory-write tool calls.
- Integration into ingestion pipeline while preserving phase-2 guarantees.

Out of scope:
- `notify` daemon integration.
- Cross-crate orchestration changes beyond `tau-memory`.

## Conformance Cases
- C-01 (AC-1, functional): watch poll executes only when directory fingerprint changes.
- C-02 (AC-2, integration): LLM mode converts tool calls into ingested memory entries.
- C-03 (AC-3, regression): reruns in LLM mode remain checkpoint-idempotent and delete-on-success safe.

## Success Metrics
- C-01..C-03 pass in scoped `tau-memory` tests.
- No duplicate chunk ingestion across watch/llm reruns.
