# Plan #2357

Status: Reviewed
Spec: specs/2357/spec.md

## Approach

1. Add a `coalescing_window_ms` configuration field to multi-channel runtime
   config and CLI wiring.
2. Introduce deterministic queue coalescing helper that groups adjacent
   transport/conversation/actor-compatible events inside the configured window.
3. Preserve traceability by tracking source event keys in each coalesced batch
   and marking all as processed.
4. Keep persisted/logged payloads deterministic by annotating coalesced metadata
   (source count and source ids) while preserving existing transport payload
   schema.
5. Add test coverage mapped to C-01..C-05 and run a live runner validation path.

## Affected Modules (planned)

- `crates/tau-multi-channel/src/multi_channel_runtime.rs`
- `crates/tau-multi-channel/src/multi_channel_runtime/tests.rs`
- `crates/tau-cli/src/cli_args.rs`
- `crates/tau-cli/src/validation.rs`
- `crates/tau-onboarding/src/startup_transport_modes.rs`

## Risks and Mitigations

- Risk: coalescing can accidentally merge unrelated events.
  - Mitigation: strict keying on transport + conversation + actor and explicit
    window threshold checks.
- Risk: dedupe regressions if only synthetic coalesced key is tracked.
  - Mitigation: track and record all original source event keys as processed.
- Risk: behavior drift in live mode.
  - Mitigation: add an integration live runner test fixture proving one coalesced
    outbound response.
