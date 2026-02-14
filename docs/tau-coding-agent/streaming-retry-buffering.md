# Streaming Retry with Buffering

## Purpose
Add bounded retries for streaming model calls while preserving partial streamed output and preventing duplicate replay across retry attempts.

## Scope
Implemented in `crates/tau-agent-core/src/lib.rs` inside the agent request retry loop.

## Runtime Behavior
- New `AgentConfig` setting:
  - `stream_retry_with_buffering` (default: `true`)
- Streaming requests now retry on retryable transport/provider errors when:
  - `request_max_retries > 0`
  - `stream_retry_with_buffering == true`

When retrying streaming calls:
1. partial deltas already emitted are buffered in retry state
2. next attempt deltas are replayed through prefix suppression
3. only new suffix content is emitted to downstream stream handlers

This preserves partial progress and avoids duplicate output such as `HelHello` after a retry.

## Fail-Closed Behavior
- If `stream_retry_with_buffering == false`, streaming requests keep prior behavior:
  - no streaming retries
  - first retryable error is returned immediately

## Compatibility
- Non-streaming request retry behavior is unchanged.
- Streaming requests that succeed on first attempt are unchanged.

## Validation Coverage
Added in `crates/tau-agent-core/src/lib.rs`:
- Unit:
  - retry buffer prefix suppression logic
- Functional:
  - retrying stream preserves output without duplication
- Integration:
  - retried streaming request continues multi-turn tool workflows
- Regression:
  - disabled buffering keeps fail-closed/no-retry streaming behavior
