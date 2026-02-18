# Plan #2507

## Approach
1. Define story/task/subtask specs with explicit G11 conformance mapping.
2. Implement any missing runtime behavior in #2509.
3. Verify via scoped tests, mutation checks, and live validation evidence.

## Risks / Mitigations
- Risk: Existing tests pass without proving typing behavior on coalesced batches.
  Mitigation: Add explicit conformance + regression tests for forced typing lifecycle on coalesced batches.

## Interfaces / Contracts
- `coalesce_inbound_events`
- `annotate_coalesced_event_metadata`
- `should_emit_typing_presence_lifecycle`
