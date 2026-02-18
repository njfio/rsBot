# Plan #2500

## Approach
1. Create accepted M86 milestone + issue artifacts for watcher + LLM extraction.
2. Deliver implementation in #2503 via strict TDD loop.
3. Record RED/GREEN evidence in #2502.

## Risks / Mitigations
- Risk: LLM extraction path could weaken deterministic ingestion behavior.
  Mitigation: preserve checkpoint semantics and enforce conformance tests for rerun safety.

## Interfaces / Contracts
- New public polling/watch APIs in `FileMemoryStore`.
- New public LLM ingestion options struct for OpenAI-compatible endpoints.
