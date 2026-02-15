# Prompt Optimization Operations Guide

This guide covers rollout-based prompt optimization mode with durable SQLite state.

This mode executes and evaluates rollouts. It is not a full reinforcement learning policy-training
pipeline.

## Run Prompt Optimization Mode

From repository root:

```bash
cargo run -p tau-coding-agent -- \
  --model openai/gpt-4o-mini \
  --prompt-optimization-config .tau/prompt-optimization.json \
  --prompt-optimization-store-sqlite .tau/training/store.sqlite \
  --prompt-optimization-json
```

`--prompt-optimization-config` switches Tau into prompt optimization mode and exits after
completion.

## Prompt Optimization Config JSON

Prompt optimization mode expects a JSON object:

```json
{
  "optimize": [
    { "prompt": "What is 2 + 2?", "expected": "4" }
  ],
  "validate": [
    { "prompt": "What is 3 + 3?", "expected": "6" }
  ],
  "resources": {
    "system_prompt": "You are a concise assistant."
  },
  "worker_count": 2,
  "poll_interval_ms": 50,
  "heartbeat_interval_ms": 1000,
  "completion_poll_interval_ms": 60,
  "completion_timeout_secs": 30
}
```

Fields:

- `optimize`: optimization rollout inputs (array of JSON objects)
- `validate`: validation rollout inputs (array of JSON objects)
- `resources`: optional initial resource snapshot persisted before execution
- `worker_count`: optional runner worker count (`> 0`)
- `poll_interval_ms`: optional rollout dequeue polling interval (`> 0`)
- `heartbeat_interval_ms`: optional worker heartbeat interval (`> 0`)
- `completion_poll_interval_ms`: optional trainer completion poll interval (`> 0`)
- `completion_timeout_secs`: optional trainer timeout (`> 0`)

At least one of `optimize` or `validate` must be non-empty.

Legacy config keys remain accepted for compatibility:

- `train` aliases `optimize`
- `val` aliases `validate`

## SQLite Store

`--prompt-optimization-store-sqlite` controls persistent state location. The store records:

- rollout queue + lifecycle status
- attempts and worker heartbeats
- execution spans
- immutable resource versions

Re-running with the same SQLite path keeps prior state for audit/inspection.

## Dashboard Metrics Export

After each successful prompt optimization run, Tau writes `.tau/training/status.json` next to the
SQLite store. This status file includes model identity and rollout outcome counters
(`total_rollouts`, `succeeded`, `failed`, `cancelled`) for dashboard/gateway status surfaces.

Gateway dashboard endpoints (`/dashboard/health`, `/dashboard/widgets`,
`/dashboard/queue-timeline`, `/dashboard/alerts`) include this status under the `training` field.

## Migration Notes

- `--train-config` -> `--prompt-optimization-config`
- `--train-store-sqlite` -> `--prompt-optimization-store-sqlite`
- `--train-json` -> `--prompt-optimization-json`

Boundary decisions and staged consolidation sets:
- `docs/guides/training-crate-boundary-plan.md`

## Ownership

Primary ownership surfaces:
- `crates/tau-trainer` (top-level orchestration lifecycle)
- `crates/tau-training-runner` and `crates/tau-training-store` (rollout execution + persistence)
- `crates/tau-algorithm` (prompt optimization strategy)
- `crates/tau-coding-agent` (CLI flag wiring and startup dispatch)

Ownership map: `docs/guides/runbook-ownership-map.md`.
