# Phase 1 Runtime Bug Map

This document maps the legacy bug IDs referenced by `#1186` to the current Tau runtime implementation.

## Bug #1: Tool calls executed serially inside a turn

- Status: fixed
- Runtime behavior:
  - tool calls execute with bounded parallelism via `AgentConfig.max_parallel_tool_calls`
  - execution path is in `crates/tau-agent-core/src/lib.rs` (`execute_tool_calls`)
- Coverage:
  - integration: `integration_parallel_tool_execution_runs_calls_concurrently_and_preserves_order`
  - regression: `regression_bug_1_max_parallel_tool_calls_zero_clamps_to_safe_serial_execution`

## Bug #3: Streaming render path blocked runtime threads

- Status: fixed
- Runtime behavior:
  - async token streaming path uses `tokio::time::sleep` in `run_prompt_with_cancellation`
  - sync fallback render path no longer applies blocking per-chunk delay
- Coverage:
  - unit: `unit_print_assistant_messages_stream_fallback_avoids_blocking_delay`
  - functional: `functional_run_prompt_with_cancellation_stream_fallback_avoids_blocking_delay`

## Bug #6: No safe parallel execution API for library consumers

- Status: fixed
- Runtime behavior:
  - `Agent::fork` clones runtime state for isolated runs
  - `Agent::run_parallel_prompts` executes bounded concurrent prompts with deterministic ordering
- Coverage:
  - integration: `integration_run_parallel_prompts_executes_runs_concurrently_with_ordered_results`
  - integration: `integration_bug_6_run_parallel_prompts_allows_zero_parallel_limit`
