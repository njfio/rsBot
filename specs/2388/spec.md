# Spec: Issue #2388 - Implement /tau react dispatch and audit logging

Status: Implemented

## Problem Statement
Tau currently supports `/tau` command execution in multi-channel runtimes, but cannot
acknowledge a message via emoji reaction. Users must send text replies even when a lightweight
reaction is more appropriate, which adds channel noise and misses Spacebot parity item G13.

## Acceptance Criteria

### AC-1 Parse `/tau react <emoji> [message_id]` as a first-class command
Given inbound text beginning with `/tau react`,
When command parsing executes,
Then it returns a typed react command containing an emoji and optional target message id.

### AC-2 React command dispatches transport reaction and suppresses text response
Given a valid react command event on a supported transport,
When `run_once_events` processes the event,
Then reaction dispatch is attempted and no outbound assistant text response is delivered.

### AC-3 React command outcomes are auditable
Given a processed react command event,
When runtime persistence completes,
Then outbound logs include deterministic command metadata (emoji, target message id,
reason/status) and reaction delivery outcome details.

### AC-4 Unsupported/invalid reaction targets fail with stable reason codes
Given a react command with an unsupported transport or invalid target id,
When the runtime processes the event,
Then command execution records failed status with stable reason codes and suppresses text
response output.

### AC-5 Existing command behavior remains unchanged
Given existing `/tau` commands (for example `/tau status` and `/tau skip`),
When processed after this change,
Then current command behavior and log contracts remain unchanged.

## Scope

### In Scope
- `MultiChannelTauCommand` parser/renderer/help updates for `react`.
- Multi-channel runtime command execution + suppression wiring for reaction commands.
- Outbound dispatcher reaction request shaping for supported transports.
- Deterministic success/failure reason codes and auditable command payload fields.

### Out of Scope
- Agent-core tool registration for autonomous LLM reaction selection.
- Slack runtime reaction path and Discord gateway websocket-native reactions.
- New transport additions beyond existing multi-channel transports.

## Conformance Cases

| Case | AC | Tier | Input | Expected |
|---|---|---|---|---|
| C-01 | AC-1 | Unit | `parse_multi_channel_tau_command("/tau react 👍 123")` | Returns `Some(React { emoji: "👍", message_id: Some("123") })` |
| C-02 | AC-1 | Unit | `render_multi_channel_tau_command_help()` | Help text lists `/tau react <emoji> [message_id]` |
| C-03 | AC-2/AC-3 | Functional | `run_once_events` with one Telegram `/tau react 👍 42` event in dry-run mode | Outbound status log has command metadata + reaction receipt; no assistant text context |
| C-04 | AC-4 | Functional | `run_once_events` with `/tau react 👍` on unsupported transport | Outbound status log includes failed reason code (stable), command payload captured, and no assistant text context |
| C-05 | AC-5 | Regression | Existing `/tau skip maintenance-window` flow | Existing skip suppression behavior remains unchanged |

## Success Metrics / Observable Signals
- C-01..C-05 tests pass in `tau-multi-channel`.
- Reaction command logs include deterministic reason/status and target metadata.
- Non-react `/tau` command tests remain green.
