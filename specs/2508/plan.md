# Plan #2508

## Approach
1. Reuse existing coalescing tests where they already map to G11.
2. Add missing test coverage for coalesced typing lifecycle behavior.
3. Keep diffs small and limited to runtime metadata/telemetry rules.

## Risks / Mitigations
- Risk: Typing signal behavior changes alter telemetry counters.
  Mitigation: Update/add precise assertions in runtime tests.

## Interfaces / Contracts
- `MultiChannelInboundEvent.metadata`
- `MultiChannelTelemetryConfig`
