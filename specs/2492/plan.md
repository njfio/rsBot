# Plan #2492

## Approach
1. Add ingestion contracts (`MemoryIngestionOptions`, `MemoryIngestionResult`).
2. Implement deterministic directory scan and supported-extension filtering.
3. Chunk files by line boundaries and derive deterministic per-chunk checkpoint keys.
4. Skip chunks already checkpointed from persisted memory records.
5. Delete files only after all chunks in that file ingest successfully.
6. Add RED tests first, then implementation, then regressions.

## Risks / Mitigations
- Risk: non-deterministic ordering causes flaky test behavior.
  Mitigation: sort file paths and process chunks in stable order.
- Risk: duplicate chunk writes under reruns.
  Mitigation: compare deterministic source-event keys against latest persisted records.

## Interfaces / Contracts
- New public ingestion API on `FileMemoryStore`.
- No new external protocol or schema changes.
