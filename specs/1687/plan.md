# Issue 1687 Plan

Status: Reviewed

## Approach

1. Add module files under `crates/tau-multi-channel/src/multi_channel_runtime/`:
   - `ingress.rs`
   - `routing.rs`
   - `outbound.rs`
2. Move ingress helper functions (live-event loading + normalization helpers) to
   `ingress.rs`.
3. Move route/health report helper functions to `routing.rs`.
4. Move outbound/retry + delivery failure helper functions to `outbound.rs`.
5. Keep root file as runtime composition layer and preserve existing function
   names through module imports.
6. Add split harness script for structural guardrails.
7. Run scoped verification for `tau-multi-channel`.

## Affected Areas

- `crates/tau-multi-channel/src/multi_channel_runtime.rs`
- `crates/tau-multi-channel/src/multi_channel_runtime/ingress.rs`
- `crates/tau-multi-channel/src/multi_channel_runtime/routing.rs`
- `crates/tau-multi-channel/src/multi_channel_runtime/outbound.rs`
- `scripts/dev/test-multi-channel-runtime-domain-split.sh`
- `specs/1687/*`

## Risks And Mitigations

- Risk: telemetry or failure logging shape drift from helper extraction.
  - Mitigation: move helper bodies verbatim and validate existing runtime tests.
- Risk: retry semantics drift.
  - Mitigation: preserve retry helper implementation unchanged and keep existing
    retry tests passing.
- Risk: symbol visibility regressions.
  - Mitigation: use `pub(super)` boundaries and strict clippy gate.

## ADR

No dependency/protocol architecture change; ADR not required.
