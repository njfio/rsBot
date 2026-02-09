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
  --github-state-dir .tau/github-issues \
  --github-poll-interval-seconds 30 \
  --github-artifact-retention-days 30
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
