# Cooperative Cancellation Tokens (Issue #1190)

Tau now propagates cancellation cooperatively across model retries, tool execution, and runtime prompt orchestration.

## What changed

- Added `CooperativeCancellationToken` to `tau-agent-core`:
  - `new()`
  - `cancel()`
  - `is_cancelled()`
- Added `Agent::set_cancellation_token(...)` to install/clear an active token for runs.
- Added `AgentError::Cancelled` to represent cooperative cancellation at the agent-runtime layer.

## Agent-core behavior

- `run_loop` checks cancellation before and between turns.
- `complete_with_retry` checks cancellation:
  - before attempts
  - while waiting on provider calls
  - during retry backoff sleeps
- tool execution now observes cancellation:
  - tool task spawning carries the active token
  - `execute_tool_call_inner` races tool execution against cancellation
  - cancellation returns deterministic tool-cancel/error payloads and propagates to run cancellation on subsequent turn boundary checks

## Runtime loop behavior

- `run_prompt_with_cancellation` now installs a cooperative token on the agent for each prompt run.
- On Ctrl+C / timeout branches:
  - token is cancelled
  - runtime gives the in-flight prompt future a short cooperative unwind window
  - checkpoint rollback is preserved
- If the prompt future returns `AgentError::Cancelled`, runtime maps it to `PromptRunStatus::Cancelled`.

## Tests added/updated

- Unit:
  - token waiter signaling (`unit_cooperative_cancellation_token_signals_waiters`)
- Functional:
  - pre-cancelled token blocks dispatch before provider call
- Integration:
  - cancellation during active tool execution propagates as `AgentError::Cancelled`
- Regression:
  - agent can continue after cancellation token is cleared

## Validation

- `cargo fmt --all`
- `cargo test -p tau-agent-core`
- `cargo test -p tau-coding-agent run_prompt_with_cancellation`
- `cargo test -p tau-runtime`
- `cargo check --workspace`
- `cargo clippy -p tau-agent-core -p tau-coding-agent -p tau-runtime --all-targets -- -D warnings`
