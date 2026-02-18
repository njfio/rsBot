# Plan #2491

## Approach
1. Add ingestion options/result contracts and ingestion function to `FileMemoryStore`.
2. Implement supported-file scan + line-chunking + deterministic chunk source keys.
3. Use existing memory persistence as durable checkpoint substrate for rerun skip logic.
4. Add tests for deterministic ingest/rerun/file lifecycle behavior.

## Risks / Mitigations
- Risk: duplicate writes on rerun.
  Mitigation: pre-load existing source-event keys and skip known chunks.
- Risk: accidental source-file loss.
  Mitigation: delete only after all chunks for a file succeed.

## Interfaces / Contracts
- New `tau-memory` ingestion API surface for one-shot directory ingestion.
- No changes to external wire formats.
