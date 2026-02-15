# M23 Doc Quality Audit Helper Report

Generated at: 2026-02-15T17:47:23Z

## Summary

- Scan root: `crates`
- Policy file: `tasks/policies/doc-quality-anti-patterns.json`
- Scanned files: `318`
- Scanned rustdoc lines: `1486`
- Findings: `37`
- Suppressed: `0`

## Findings

| Pattern | Path | Line | Comment |
| --- | --- | ---: | --- |
| generic_sets_gets_returns | crates/tau-agent-core/src/lib.rs | 200 | Returns true when cancellation has been requested. |
| generic_sets_gets_returns | crates/tau-agent-core/src/lib.rs | 465 | Returns true when the policy allows direct messages from `from_agent_id` to `to_agent_id`. |
| generic_sets_gets_returns | crates/tau-agent-core/src/lib.rs | 639 | Returns aggregate async event dispatch metrics for this agent instance. |
| generic_sets_gets_returns | crates/tau-agent-core/src/lib.rs | 724 | Returns true when a tool with `tool_name` is registered. |
| generic_sets_gets_returns | crates/tau-agent-core/src/lib.rs | 729 | Returns sorted registered tool names. |
| generic_sets_gets_returns | crates/tau-agent-core/src/lib.rs | 754 | Returns this agent's identifier for policy checks and direct messaging. |
| generic_sets_gets_returns | crates/tau-agent-core/src/lib.rs | 759 | Sets this agent's identifier when `agent_id` is non-empty after trimming. |
| generic_sets_gets_returns | crates/tau-agent-core/src/lib.rs | 898 | Returns the active runtime safety policy. |
| generic_sets_gets_returns | crates/tau-agent-core/src/lib.rs | 913 | Returns the current conversation history. |
| generic_sets_gets_returns | crates/tau-agent-core/src/lib.rs | 944 | Returns cumulative token usage and estimated model spend for this agent. |
| generic_sets_gets_returns | crates/tau-core/src/time_utils.rs | 1 | Returns the current Unix timestamp in milliseconds. |
| generic_sets_gets_returns | crates/tau-core/src/time_utils.rs | 11 | Returns the current Unix timestamp in seconds. |
| generic_sets_gets_returns | crates/tau-core/src/time_utils.rs | 19 | Returns true when `expires_unix` is present and no longer in the future. |
| generic_sets_gets_returns | crates/tau-memory/src/runtime.rs | 117 | Returns true when `scope` satisfies the configured filter dimensions. |
| generic_sets_gets_returns | crates/tau-memory/src/runtime.rs | 308 | Returns the store root directory. |
| generic_sets_gets_returns | crates/tau-memory/src/runtime.rs | 313 | Returns the active storage backend. |
| generic_sets_gets_returns | crates/tau-memory/src/runtime.rs | 318 | Returns the active storage backend label. |
| generic_sets_gets_returns | crates/tau-memory/src/runtime.rs | 323 | Returns the backend selection reason code. |
| generic_sets_gets_returns | crates/tau-memory/src/runtime.rs | 328 | Returns the resolved storage file path, when applicable. |
| generic_sets_gets_returns | crates/tau-memory/src/runtime.rs | 430 | Returns latest records filtered by scope and bounded by `limit`. |
| generic_sets_gets_returns | crates/tau-memory/src/runtime.rs | 767 | Returns a hierarchical workspace/channel/actor tree for latest records. |
| generic_sets_gets_returns | crates/tau-runtime/src/background_jobs_runtime.rs | 73 | Returns the stable snake_case wire representation. |
| generic_sets_gets_returns | crates/tau-runtime/src/background_jobs_runtime.rs | 84 | Returns true when the job cannot transition any further. |
| generic_sets_gets_returns | crates/tau-runtime/src/background_jobs_runtime.rs | 308 | Returns the configured runtime state directory. |
| generic_sets_gets_returns | crates/tau-runtime/src/background_jobs_runtime.rs | 313 | Returns the persisted health snapshot path. |
| generic_sets_gets_returns | crates/tau-runtime/src/background_jobs_runtime.rs | 318 | Returns the append-only event log path. |
| generic_sets_gets_returns | crates/tau-runtime/src/background_jobs_runtime.rs | 323 | Returns the directory containing per-job manifests and outputs. |
| generic_sets_gets_returns | crates/tau-runtime/src/background_jobs_runtime.rs | 432 | Returns the latest runtime health counters snapshot. |
| generic_sets_gets_returns | crates/tau-startup/src/startup_safety_policy.rs | 21 | Returns the canonical safety-policy precedence contract. |
| generic_sets_gets_returns | crates/tau-tools/src/tools/registry_core.rs | 569 | Returns the reserved registry of built-in agent tool names. |
| generic_sets_gets_returns | crates/tau-training-tracer/src/lib.rs | 289 | Returns an in-memory snapshot of completed spans. |
| generic_sets_gets_returns | crates/tau-training-types/src/lib.rs | 101 | Returns true when this status can transition to `next`. |
| generic_sets_gets_returns | crates/tau-training-types/src/lib.rs | 129 | Returns an error if transitioning to `next` is not allowed. |
| generic_sets_gets_returns | crates/tau-training-types/src/lib.rs | 142 | Returns true when no further execution is expected. |
| generic_sets_gets_returns | crates/tau-training-types/src/lib.rs | 161 | Returns true when this status can transition to `next`. |
| generic_sets_gets_returns | crates/tau-training-types/src/lib.rs | 181 | Returns an error if transitioning to `next` is not allowed. |
| generic_sets_gets_returns | crates/tau-training-types/src/lib.rs | 194 | Returns true when no further attempt work is expected. |

## Suppressed

| Suppression ID | Pattern | Path | Line |
| --- | --- | --- | ---: |
