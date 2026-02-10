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
