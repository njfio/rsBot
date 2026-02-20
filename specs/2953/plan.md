# Plan: Issue #2953 - Wire `/cortex/chat` to LLM with observer context and fallback

## Approach
1. Add RED tests validating that `/cortex/chat` no longer relies on mock char-count output and that fallback behavior is deterministic.
2. Implement bounded context builders:
- observer status summary from persisted observer events,
- bulletin snapshot from `state.cortex`,
- memory graph summary from memory-store records.
3. Build `ChatRequest` and invoke `state.config.client.complete(...)`.
4. Convert response to bounded text; use deterministic fallback on error/empty output.
5. Preserve SSE event contract and extend metadata with reason code.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses/cortex_runtime.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`
- `specs/milestones/m168/index.md`
- `specs/2953/spec.md`
- `specs/2953/tasks.md`

## Risks / Mitigations
- Risk: provider call failure causes endpoint instability.
  - Mitigation: always degrade to deterministic fallback and keep HTTP/SSE success contract.
- Risk: oversized prompt context.
  - Mitigation: bounded event/memory summaries and explicit character truncation.
- Risk: regression to SSE contract used by UI.
  - Mitigation: preserve event names/order and add targeted conformance tests.

## Interfaces / Contracts
- Keep endpoint path and auth behavior unchanged.
- Preserve SSE event sequence and payload envelope.
- Add reason-code metadata for LLM/fallback classification.

## ADR
No ADR required: no dependency additions or architecture boundary changes.
