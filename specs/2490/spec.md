# Spec #2490 - G9 memory ingestion phase-1 orchestration

Status: Implemented

## Problem Statement
Tau lacks first-class bulk memory ingestion from workspace files. We need a bounded, testable phase-1 slice that introduces deterministic ingestion foundations without broad runtime architecture changes.

## Acceptance Criteria
### AC-1 Phase-1 scope is explicit and bounded
Given M84 execution, when work is implemented, then scope is limited to one-shot ingestion API, line-chunking, durable checkpoints, rerun safety, and file lifecycle handling.

### AC-2 Child issues provide full conformance traceability
Given #2491/#2492/#2493, when work completes, then AC-to-conformance-to-test mappings and RED/GREEN evidence are present.

## Scope
In scope:
- Milestone and issue artifact chain for M84.
- `tau-memory` ingestion API and tests.

Out of scope:
- Continuous watcher workers.
- LLM-mediated chunk synthesis.
- New external ingestion protocols.

## Conformance Cases
- C-01 (AC-1, governance): milestone + child specs define bounded scope.
- C-02 (AC-2, governance): #2492/#2493 include mapped conformance and evidence.

## Success Metrics
- M84 issues close with status `done`.
- #2492 AC matrix has no failing entries.
