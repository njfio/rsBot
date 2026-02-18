# Plan #2509

## Approach
1. Add RED tests for C-01..C-04 (reusing and renaming/adding where needed).
2. Add minimal runtime metadata wiring: force typing lifecycle for coalesced batches.
3. Run scoped verify gates (`fmt`, `clippy`, `cargo test -p tau-multi-channel -- spec_2509`).
4. Run mutation checks for changed diff and live validation script.

## Risks / Mitigations
- Risk: Forced typing lifecycle may inflate telemetry unexpectedly.
  Mitigation: Scope forcing to explicit coalesced batch metadata (`batch_size > 1`) and assert expected counters.

## Interfaces / Contracts
- `coalesce_inbound_events`
- `annotate_coalesced_event_metadata`
- `should_emit_typing_presence_lifecycle`
