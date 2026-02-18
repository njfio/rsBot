# Plan #2496

## Approach
1. Add worker-oriented wrapper entrypoint over ingestion execution.
2. Introduce SHA-256 checkpoint digest generation for ingestion chunks.
3. Persist and query checkpoint records from SQLite for rerun skipping.
4. Add conformance tests validating worker execution, digest format, and rerun behavior.

## Risks / Mitigations
- Risk: legacy phase-1 checkpoints become ineffective.
  Mitigation: preserve compatibility by still recognizing prior persisted checkpoint keys.
- Risk: checkpoint writes diverge from memory writes.
  Mitigation: write checkpoint only after successful chunk memory persistence.

## Interfaces / Contracts
- `tau-memory` ingestion API additions only.
- SQLite schema extension limited to ingestion checkpoint table.
