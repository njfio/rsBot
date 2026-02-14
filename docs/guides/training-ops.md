# Training Operations Guide

This guide covers rollout-based training mode with durable SQLite state.

## Run Training Mode

From repository root:

```bash
cargo run -p tau-coding-agent -- \
  --model openai/gpt-4o-mini \
  --train-config examples/training/train.json \
  --train-store-sqlite .tau/training/store.sqlite \
  --train-json
```

`--train-config` switches Tau into training mode and exits after completion.

## Training Config JSON

Training mode expects a JSON object:

```json
{
  "train": [
    { "prompt": "What is 2 + 2?", "expected": "4" }
  ],
  "val": [
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

- `train`: training rollout inputs (array of JSON objects)
- `val`: validation rollout inputs (array of JSON objects)
- `resources`: optional initial resource snapshot persisted before execution
- `worker_count`: optional runner worker count (`> 0`)
- `poll_interval_ms`: optional rollout dequeue polling interval (`> 0`)
- `heartbeat_interval_ms`: optional worker heartbeat interval (`> 0`)
- `completion_poll_interval_ms`: optional trainer completion poll interval (`> 0`)
- `completion_timeout_secs`: optional trainer timeout (`> 0`)

At least one of `train` or `val` must be non-empty.

## SQLite Store

`--train-store-sqlite` controls persistent state location. The store records:

- rollout queue + lifecycle status
- attempts and worker heartbeats
- execution spans
- immutable resource versions

Re-running with the same SQLite path keeps prior state for audit/inspection.
