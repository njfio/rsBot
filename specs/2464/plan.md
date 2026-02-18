# Plan #2464 - runtime heartbeat policy hot-reload without restart

## Approach
1. Add sidecar policy file discovery for heartbeat state path.
2. Reload policy only when file fingerprint changes.
3. Apply supported policy fields to active scheduler state.
4. Emit deterministic diagnostics/reason codes for applied/invalid reload events.
5. Cover behavior with conformance and regression tests.

## Risks
- Risk: unstable tests due timing.
  - Mitigation: bounded polling and interval values with deterministic assertions.
- Risk: malformed policy file causing runtime failure.
  - Mitigation: fail-closed to last-known-good config and record reason code.

## Interfaces/Contracts
- Sidecar policy JSON shape (phase-1): `{ "interval_ms": <u64> }`.
