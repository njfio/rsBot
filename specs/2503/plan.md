# Plan #2503

## Approach
1. Add RED tests for C-01..C-05.
2. Introduce public polling state and LLM options structs.
3. Refactor ingestion loop into a shared internal path supporting direct and LLM chunk processors.
4. Add OpenAI-compatible chat completions request/response parsing for `memory_write` tool calls.
5. Preserve checkpoint persistence and file lifecycle guarantees from phase-2.

## Risks / Mitigations
- Risk: LLM output schema drift introduces ingest instability.
  Mitigation: strict parser + explicit diagnostics and regression tests.
- Risk: watch polling misses transitions.
  Mitigation: deterministic fingerprint map over sorted files with metadata-based signatures.

## Interfaces / Contracts
- `FileMemoryStore::ingest_directory_worker_once_with_llm_memory_save`
- `FileMemoryStore::ingest_directory_watch_poll_once`
- `FileMemoryStore::ingest_directory_watch_poll_once_with_llm_memory_save`
