# Plan #2490

## Approach
1. Create M84 hierarchy and accepted artifacts.
2. Deliver implementation in #2492 with strict TDD.
3. Validate with scoped quality gates and live validation.

## Risks / Mitigations
- Risk: scope drift into full ingestion architecture.
  Mitigation: keep implementation bounded to one-shot API and explicit out-of-scope list.

## Interfaces / Contracts
- No external protocol change in this phase.
- Internal contract introduced in `tau-memory` for deterministic ingestion execution.
