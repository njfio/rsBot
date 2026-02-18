# Spec #2495 - G9 phase-2 ingestion worker and SHA-256 checkpoints

Status: Accepted

## Problem Statement
Phase-1 ingestion (`#2492`) established deterministic one-shot chunk ingestion, but it does not provide a worker-oriented execution surface or SHA-256 checkpoint durability aligned with the remaining G9 gap.

## Acceptance Criteria
### AC-1 Phase-2 scope is explicit and bounded
Given M85 execution, when work is implemented, then scope is limited to worker-oriented ingestion execution, SHA-256 checkpointing, durable rerun safety, and conformance coverage.

### AC-2 Child issues preserve deterministic behavior
Given #2496/#2497/#2498, when work completes, then rerun idempotency, diagnostics, and file lifecycle behavior are validated with mapped conformance tests.

## Scope
In scope:
- Milestone and issue artifact chain for M85.
- `tau-memory` worker ingestion entrypoint and SHA-256 checkpoint tracking.

Out of scope:
- New ingestion transports or watcher daemons.
- Unrelated memory retrieval/ranking changes.

## Conformance Cases
- C-01 (AC-1, governance): M85 + child specs define bounded scope and explicit outputs.
- C-02 (AC-2, governance): #2497 conformance tests prove deterministic rerun and diagnostics behavior.

## Success Metrics
- M85 issues close with status `done`.
- #2497 AC matrix has no failing entries.
