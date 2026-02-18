# M84 - Spacebot G9 Memory Ingestion (Phase 1)

Milestone: GitHub milestone `M84 - Spacebot G9 Memory Ingestion (Phase 1)`

## Objective
Deliver a bounded first production slice of `tasks/spacebot-comparison.md` gap `G9` by adding deterministic bulk ingestion foundations in `tau-memory`.

## Scope
- Add a deterministic one-shot ingestion API in `tau-memory`.
- Scan workspace ingest directory for supported file types.
- Chunk file content on line boundaries.
- Persist deterministic per-chunk checkpoints using durable memory records.
- Support crash-resilient reruns by skipping previously ingested chunks.
- Delete source files only after full successful ingestion.
- Add conformance/regression tests and RED/GREEN evidence.

## Out of Scope
- LLM summarization of chunks into higher-order memories.
- Background daemon/watcher scheduling of ingestion runs.
- New gateway/transport ingestion APIs.

## Issue Hierarchy
- Epic: #2490
- Story: #2491
- Task: #2492
- Subtask: #2493

## Exit Criteria
- ACs for #2492 are verified by conformance tests.
- RED/GREEN evidence for #2493 is recorded.
- `cargo fmt --check`, `cargo clippy -p tau-memory -- -D warnings`, and scoped tests pass.
