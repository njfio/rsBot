# Plan #2497

## Approach
1. Add RED tests for C-01..C-04 in `tau-memory` runtime query tests.
2. Implement worker ingestion entrypoint that delegates to existing ingestion path.
3. Replace checkpoint digest generation with SHA-256.
4. Add durable SQLite checkpoint load/write helpers and integrate with rerun skip logic.
5. Preserve failure diagnostics and file retention semantics from phase-1.

## Risks / Mitigations
- Risk: introducing checkpoint table causes backend divergence.
  Mitigation: limit table usage to SQLite backend with deterministic fallback from existing source-event keys.
- Risk: hash migration breaks deterministic memory IDs.
  Mitigation: assert digest shape and stable memory-id prefix logic in unit tests.

## Interfaces / Contracts
- New public worker ingestion API on `FileMemoryStore`.
- SQLite schema extension for ingestion checkpoints.
