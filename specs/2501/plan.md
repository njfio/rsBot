# Plan #2501

## Approach
1. Add watch-poll state + fingerprint detection APIs in `FileMemoryStore`.
2. Add LLM ingestion options and OpenAI-compatible chat-completions parser for `memory_write` tool calls.
3. Refactor one-shot ingestion into a shared internal path supporting direct and LLM modes.
4. Add conformance/regression tests for polling behavior, LLM extraction, and rerun idempotency.

## Risks / Mitigations
- Risk: malformed tool-call payloads could silently ingest bad data.
  Mitigation: strict argument parsing with explicit diagnostics and fail-safe file retention.
- Risk: polling state could miss file changes.
  Mitigation: stable per-file fingerprint map (path + metadata) and conformance tests for changed/unchanged cycles.

## Interfaces / Contracts
- New `MemoryIngestionLlmOptions` and `MemoryIngestionWatchPollingState` public structs.
- New worker/watch methods instrumented with `tracing`.
