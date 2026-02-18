# Spec #2500 - G9 phase-3 watcher polling + LLM chunk memory extraction

Status: Accepted

## Problem Statement
G9 still lacks two required capabilities: continuous ingest directory watching and chunk processing through an LLM memory-save tool pathway.

## Acceptance Criteria
### AC-1 Phase-3 scope is explicit and bounded
Given M86 execution, when work is implemented, then scope is limited to watcher polling, LLM chunk extraction via memory-save semantics, and conformance coverage.

### AC-2 Child issues preserve deterministic ingestion safety
Given #2501/#2503/#2502, when work completes, then checkpoint idempotency, file lifecycle guarantees, and chunk extraction diagnostics remain verifiable and deterministic.

## Scope
In scope:
- Milestone and issue artifact chain for M86.
- Polling watcher API and LLM extraction path in `tau-memory`.

Out of scope:
- Non-polling file watcher daemons.
- Unrelated memory retrieval/ranking changes.

## Conformance Cases
- C-01 (AC-1, governance): M86 and child specs define bounded watcher + LLM scope.
- C-02 (AC-2, governance): #2503 conformance tests verify poll/watch and LLM extraction behavior.

## Success Metrics
- M86 issues close with status `done`.
- #2503 AC matrix has no failing entries.
