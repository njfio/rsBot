# M86 - Spacebot G9 Memory Ingestion (Phase 3)

Milestone: GitHub milestone `M86 - Spacebot G9 Memory Ingestion (Phase 3)`

## Objective
Complete the remaining `tasks/spacebot-comparison.md` G9 gaps by adding heartbeat-style ingest directory watch polling and chunk processing through an LLM-driven `memory_write`/`memory_save` extraction pipeline.

## Scope
- Add polling watch APIs in `tau-memory` for ingest directory change detection.
- Add OpenAI-compatible LLM chunk extraction path that consumes `memory_write` tool calls.
- Preserve SHA-256 checkpoint idempotency and delete-on-success lifecycle guarantees.
- Add conformance + regression tests with RED/GREEN evidence.

## Out of Scope
- `notify`-based OS file watching daemons.
- New external ingestion gateway endpoints.
- Retrieval/ranking changes unrelated to ingestion.

## Issue Hierarchy
- Epic: #2500
- Story: #2501
- Task: #2503
- Subtask: #2502

## Exit Criteria
- ACs for #2503 are verified by conformance tests.
- RED/GREEN evidence for #2502 is recorded.
- `cargo fmt --check`, `cargo clippy -p tau-memory -- -D warnings`, and scoped tests pass.
