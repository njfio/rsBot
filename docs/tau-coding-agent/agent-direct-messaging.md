# Agent-to-Agent Direct Messaging

## Purpose
Enable direct messaging between agents with explicit policy boundaries and fail-closed behavior.

## Scope
Implemented in `crates/tau-agent-core/src/lib.rs`.

## New Types and APIs
- `AgentDirectMessagePolicy`
  - allowlist-based route policy
  - supports directional and bidirectional route grants
  - supports configurable max message size
- `AgentDirectMessageError`
  - unauthorized route
  - empty message
  - oversized message
- `Agent` additions:
  - `agent_id()` / `set_agent_id(...)`
  - `send_direct_message(...)`
  - `receive_direct_message(...)`

## Policy Boundaries
Direct messaging is denied unless policy explicitly allows `from -> to`.

Default policy behavior:
- self-messages disabled
- no routes allowed
- max message chars = 4000

## Message Injection Model
Authorized direct messages are appended to the recipient as system messages in this format:
- `[Tau direct message] from=<sender> to=<recipient>`
- message body

This keeps messaging explicit and auditable in prompt context.

## Compatibility
- Existing prompt/tool flows remain unchanged.
- Direct messaging is opt-in through policy configuration.

## Validation Coverage
Added in `crates/tau-agent-core/src/lib.rs`:
- Unit:
  - route policy semantics
- Functional:
  - direct message append behavior
- Integration:
  - direct message appears in recipient prompt context
- Regression:
  - unauthorized and oversized messages fail closed without mutating recipient state
