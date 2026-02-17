# Plan: Issue #2388 - Implement /tau react dispatch and audit logging

## Approach
1. Extend `MultiChannelTauCommand` with a `React` variant and parser/help/renderer support.
2. Extend `MultiChannelCommandExecution` with reaction metadata (emoji + target message id).
3. Add outbound dispatcher API for reactions with transport-specific payload shaping.
4. In `persist_event`, execute reaction delivery on suppressed command path and log deterministic
   success/failure payloads.
5. Add conformance tests first (parser/help/functional success/failure/regression) then implement.

## Affected Modules
- `crates/tau-multi-channel/src/multi_channel_runtime.rs`
- `crates/tau-multi-channel/src/multi_channel_runtime/tests.rs`
- `crates/tau-multi-channel/src/multi_channel_outbound.rs`

## Risks and Mitigations
- Risk: transport-specific reaction APIs differ from message send APIs.
  - Mitigation: keep initial support narrow and deterministic with explicit reason codes for
    unsupported transports/invalid ids.
- Risk: command suppression branch could regress skip behavior.
  - Mitigation: preserve existing branch shape and add explicit regression coverage (C-05).
- Risk: mutation escapes around conditional log fields.
  - Mitigation: assert exact payload fields (`emoji`, `message_id`, `reason_code`, status).

## Interface/Contract Notes
- New command syntax: `/tau react <emoji> [message_id]`.
- Command payload schema remains `multi_channel_tau_command_v1` with added optional fields:
  `react_emoji`, `react_message_id`.
- Outbound log entry continues to use `direction=outbound` and stable `status` field.

## ADR
- No new dependency or protocol format introduced; ADR not required for this slice.
