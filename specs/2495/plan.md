# Plan #2495

## Approach
1. Create accepted M85 milestone + issue artifacts.
2. Deliver phase-2 implementation in #2497 with strict TDD.
3. Validate with scoped quality gates and recorded RED/GREEN evidence in #2498.

## Risks / Mitigations
- Risk: checkpoint migration breaks rerun idempotency.
  Mitigation: include compatibility checks for previously persisted phase-1 keys.
- Risk: scope creep into full ingestion orchestration architecture.
  Mitigation: keep this phase bounded to worker entrypoint + checkpoint durability.

## Interfaces / Contracts
- Internal `tau-memory` ingestion contracts only.
- No external wire/protocol changes in this phase.
