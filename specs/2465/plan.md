# Plan #2465 - add heartbeat scheduler policy reload path + conformance tests

## Approach
1. Add sidecar policy path resolver in `tau-runtime` heartbeat module.
2. Track policy file fingerprint (`modified`, `len`) and only reload on change.
3. Parse and validate policy JSON (`interval_ms > 0`).
4. Apply valid interval to active scheduler loop without restart.
5. Record deterministic reload diagnostics/reason codes for applied/invalid policy events.
6. Add RED tests for C-01..C-03, then implement GREEN and regression verification.

## Affected Modules
- `crates/tau-runtime/src/heartbeat_runtime.rs`

## Risks
- Timing-sensitive tests around interval transition.
  - Mitigation: short bounded intervals and polling helper assertions.
- Reload noise emitted every tick.
  - Mitigation: emit reload reasons only on actual policy-file change events.

## Interfaces/Contracts
- Sidecar policy file path: `<state_path>.policy.json` (phase-1 convention).
- Policy payload: `{ "interval_ms": <u64> }`.
