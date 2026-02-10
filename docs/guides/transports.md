# Transport Guide

Run all commands from repository root.

## GitHub Issues bridge

```bash
export GITHUB_TOKEN=...your-token...

cargo run -p tau-coding-agent -- \
  --model openai/gpt-4o-mini \
  --github-issues-bridge \
  --github-repo owner/repo \
  --github-bot-login your-bot-login \
  --github-required-label tau-ready \
  --github-issue-number 7 \
  --github-state-dir .tau/github-issues \
  --github-poll-interval-seconds 30 \
  --github-artifact-retention-days 30
```

`--github-required-label` can be repeated; only issues with at least one matching label are processed.
`--github-issue-number` can be repeated; only matching issue numbers are processed.

Run exactly one poll cycle (useful for CI smoke jobs and cron workflows):

```bash
cargo run -p tau-coding-agent -- \
  --model openai/gpt-4o-mini \
  --github-issues-bridge \
  --github-repo owner/repo \
  --github-poll-once \
  --github-state-dir .tau/github-issues
```

Bridge control commands in issue comments:

- `/tau help`
- `/tau status`
- `/tau health`
- `/tau stop`
- `/tau chat start|resume|reset|status|show|search|export`
- `/tau artifacts|artifacts run <run_id>|artifacts show <artifact_id>|artifacts purge`

## Slack Socket Mode bridge

```bash
export TAU_SLACK_APP_TOKEN=...xapp-token...
export TAU_SLACK_BOT_TOKEN=...xoxb-token...

cargo run -p tau-coding-agent -- \
  --model openai/gpt-4o-mini \
  --slack-bridge \
  --slack-state-dir .tau/slack \
  --slack-artifact-retention-days 30 \
  --slack-thread-detail-output true \
  --slack-thread-detail-threshold-chars 1500
```

## Multi-channel contract runner (Telegram, Discord, WhatsApp)

Use this fixture-driven runtime mode to validate channel-store writes, retry behavior, and
deduplication for supported transports.

```bash
cargo run -p tau-coding-agent -- \
  --model openai/gpt-4o-mini \
  --multi-channel-contract-runner \
  --multi-channel-fixture crates/tau-coding-agent/testdata/multi-channel-contract/baseline-three-channel.json \
  --multi-channel-state-dir .tau/multi-channel \
  --multi-channel-queue-limit 64 \
  --multi-channel-processed-event-cap 10000 \
  --multi-channel-retry-max-attempts 4 \
  --multi-channel-retry-base-delay-ms 0
```

The runner writes channel-store output under:

- `.tau/multi-channel/channel-store/telegram/...`
- `.tau/multi-channel/channel-store/discord/...`
- `.tau/multi-channel/channel-store/whatsapp/...`

Runtime state for duplicate suppression is persisted at:

- `.tau/multi-channel/state.json`
- `.tau/multi-channel/runtime-events.jsonl` (per-cycle observability log)

Inspect multi-channel transport health snapshot:

```bash
cargo run -p tau-coding-agent -- \
  --multi-channel-state-dir .tau/multi-channel \
  --transport-health-inspect multi-channel \
  --transport-health-json
```

Operational rollout and rollback guidance: `docs/guides/multi-channel-ops.md`.

## Multi-agent contract runner

Use this fixture-driven runtime mode to validate planner/delegated/review route selection,
retry handling, deduplication, and routed-case snapshot persistence.

```bash
cargo run -p tau-coding-agent -- \
  --model openai/gpt-4o-mini \
  --multi-agent-contract-runner \
  --multi-agent-fixture crates/tau-coding-agent/testdata/multi-agent-contract/rollout-pass.json \
  --multi-agent-state-dir .tau/multi-agent \
  --multi-agent-queue-limit 64 \
  --multi-agent-processed-case-cap 10000 \
  --multi-agent-retry-max-attempts 4 \
  --multi-agent-retry-base-delay-ms 0
```

The runner writes state and observability output under:

- `.tau/multi-agent/state.json`
- `.tau/multi-agent/runtime-events.jsonl`
- `.tau/multi-agent/channel-store/multi-agent/orchestrator-router/...`

Inspect multi-agent transport health snapshot:

```bash
cargo run -p tau-coding-agent -- \
  --multi-agent-state-dir .tau/multi-agent \
  --transport-health-inspect multi-agent \
  --transport-health-json
```

Inspect multi-agent rollout guardrail/status report:

```bash
cargo run -p tau-coding-agent -- \
  --multi-agent-state-dir .tau/multi-agent \
  --multi-agent-status-inspect \
  --multi-agent-status-json
```

Operational rollout and rollback guidance: `docs/guides/multi-agent-ops.md`.

## Semantic memory contract runner

Use this fixture-driven runtime mode to validate semantic memory extraction/retrieval
processing, retry handling, and channel-store snapshot writes.

```bash
cargo run -p tau-coding-agent -- \
  --model openai/gpt-4o-mini \
  --memory-contract-runner \
  --memory-fixture crates/tau-coding-agent/testdata/memory-contract/mixed-outcomes.json \
  --memory-state-dir .tau/memory \
  --memory-queue-limit 64 \
  --memory-processed-case-cap 10000 \
  --memory-retry-max-attempts 4 \
  --memory-retry-base-delay-ms 0
```

The runner writes state and observability output under:

- `.tau/memory/state.json`
- `.tau/memory/runtime-events.jsonl`
- `.tau/memory/channel-store/memory/<channel_id>/...`

Inspect semantic memory health snapshot:

```bash
cargo run -p tau-coding-agent -- \
  --memory-state-dir .tau/memory \
  --transport-health-inspect memory \
  --transport-health-json
```

Operational rollout and rollback guidance: `docs/guides/memory-ops.md`.

## Dashboard contract runner

Use this fixture-driven runtime mode to validate dashboard state transitions, control actions,
retry handling, and channel-store snapshot writes.

```bash
cargo run -p tau-coding-agent -- \
  --model openai/gpt-4o-mini \
  --dashboard-contract-runner \
  --dashboard-fixture crates/tau-coding-agent/testdata/dashboard-contract/mixed-outcomes.json \
  --dashboard-state-dir .tau/dashboard \
  --dashboard-queue-limit 64 \
  --dashboard-processed-case-cap 10000 \
  --dashboard-retry-max-attempts 4 \
  --dashboard-retry-base-delay-ms 0
```

The runner writes state and observability output under:

- `.tau/dashboard/state.json`
- `.tau/dashboard/runtime-events.jsonl`
- `.tau/dashboard/channel-store/dashboard/<channel_id>/...`

Inspect dashboard transport health snapshot:

```bash
cargo run -p tau-coding-agent -- \
  --dashboard-state-dir .tau/dashboard \
  --transport-health-inspect dashboard \
  --transport-health-json
```

Inspect dashboard rollout guardrail/status report:

```bash
cargo run -p tau-coding-agent -- \
  --dashboard-state-dir .tau/dashboard \
  --dashboard-status-inspect \
  --dashboard-status-json
```

Operational rollout and rollback guidance: `docs/guides/dashboard-ops.md`.

## Gateway contract runner

Use this fixture-driven runtime mode to validate Tau gateway request handling, retry outcomes,
state persistence, and channel-store snapshots.

```bash
cargo run -p tau-coding-agent -- \
  --model openai/gpt-4o-mini \
  --gateway-contract-runner \
  --gateway-fixture crates/tau-coding-agent/testdata/gateway-contract/rollout-pass.json \
  --gateway-state-dir .tau/gateway
```

The runner writes state and observability output under:

- `.tau/gateway/state.json`
- `.tau/gateway/runtime-events.jsonl`
- `.tau/gateway/channel-store/gateway/<actor_id>/...`

Inspect gateway transport health snapshot:

```bash
cargo run -p tau-coding-agent -- \
  --gateway-state-dir .tau/gateway \
  --transport-health-inspect gateway \
  --transport-health-json
```

Inspect gateway rollout guardrail/status report:

```bash
cargo run -p tau-coding-agent -- \
  --gateway-state-dir .tau/gateway \
  --gateway-status-inspect \
  --gateway-status-json
```

Operational rollout and rollback guidance: `docs/guides/gateway-ops.md`.

## ChannelStore inspection and repair

Inspect one channel:

```bash
cargo run -p tau-coding-agent -- \
  --channel-store-root .tau/channel-store \
  --channel-store-inspect github/issue-9
```

Repair malformed JSONL lines for one channel:

```bash
cargo run -p tau-coding-agent -- \
  --channel-store-root .tau/channel-store \
  --channel-store-repair slack/C123
```

## RPC protocol commands

Capabilities:

```bash
cargo run -p tau-coding-agent -- --rpc-capabilities
```

Validate one frame:

```bash
cargo run -p tau-coding-agent -- --rpc-validate-frame-file /tmp/rpc-frame.json
```

Dispatch one frame:

```bash
cargo run -p tau-coding-agent -- --rpc-dispatch-frame-file /tmp/rpc-frame.json
```

Dispatch NDJSON file:

```bash
cargo run -p tau-coding-agent -- --rpc-dispatch-ndjson-file /tmp/rpc-frames.ndjson
```

Serve long-lived NDJSON over stdin/stdout:

```bash
cat /tmp/rpc-frames.ndjson | cargo run -p tau-coding-agent -- --rpc-serve-ndjson
```

RPC schema compatibility fixtures live under `crates/tau-coding-agent/testdata/rpc-schema-compat/`.
