# M85 - Spacebot G9 Memory Ingestion (Phase 2)

Milestone: GitHub milestone `M85 - Spacebot G9 Memory Ingestion (Phase 2)`

## Objective
Complete the next bounded `tasks/spacebot-comparison.md` G9 slice by adding worker-oriented ingestion execution and durable SHA-256 chunk checkpoint tracking.

## Scope
- Add a worker-style ingestion entrypoint in `tau-memory` that executes one-shot ingestion runs.
- Replace phase-1 chunk checkpoint material hashing with deterministic SHA-256 digests.
- Persist ingestion chunk checkpoints in SQLite for crash-resilient reruns.
- Preserve deterministic rerun skip behavior and file lifecycle guarantees from phase-1.
- Add conformance/regression tests with RED/GREEN evidence.

## Out of Scope
- Continuous filesystem watcher or daemon scheduling.
- LLM summarization/extraction beyond existing chunk-to-fact ingestion.
- New external gateway ingestion APIs.

## Issue Hierarchy
- Epic: #2495
- Story: #2496
- Task: #2497
- Subtask: #2498

## Exit Criteria
- ACs for #2497 are verified by conformance tests.
- RED/GREEN evidence for #2498 is recorded.
- `cargo fmt --check`, `cargo clippy -p tau-memory -- -D warnings`, and scoped tests pass.
