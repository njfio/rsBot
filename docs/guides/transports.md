# Transport Guide

Run all commands from repository root.

Unified operator control-plane snapshot (all core transport/runtime surfaces):

```bash
cargo run -p tau-coding-agent -- \
  --operator-control-summary \
  --operator-control-summary-json
```

Troubleshooting map and field details: `docs/guides/operator-control-summary.md`.

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
- `/tau auth <status|matrix> ...`
- `/tau doctor [--online]`
- `/tau stop`
- `/tau chat start|resume|reset|status|summary|replay|show|search|export`
- `/tau artifacts|artifacts run <run_id>|artifacts show <artifact_id>|artifacts purge`
- `/tau demo-index list|run [scenario[,scenario...]] [--timeout-seconds <n>]|report`

Issue command response schema (normalized):
- Footer format: ``Tau command `<command>` | status `<status>` | reason_code `<reason_code>` ``.
- Status taxonomy:
- `acknowledged`: command accepted (for example stop/cancel acknowledgement).
- `reported`: command completed and returned diagnostics/control output.
- `failed`: command execution failed.
- Artifact pointers: `label: id=\`...\` path=\`...\` bytes=\`...\``.
- Oversized command output is truncated deterministically and spilled to a channel-store artifact:
- `output_truncated: true`
- `overflow_artifact: id=\`...\` path=\`...\` bytes=\`...\``

Auth diagnostics commands:
- `/tau auth status`: report provider auth posture with strict-subscription context.
- `/tau auth matrix`: report cross-provider mode/availability matrix and filters.

Doctor diagnostics command:
- `/tau doctor`: run bounded local diagnostics and post summary plus artifact pointers.
- `/tau doctor --online`: include remote release-update lookup (network dependent).

Demo-index commands for issue-driven demos:
- `/tau demo-index list`: show allowlisted scenarios and expected markers.
- `/tau demo-index run onboarding,gateway-auth --timeout-seconds 120`: execute bounded demo scenarios from the issue thread and persist report/log artifacts.
- `/tau demo-index report`: show latest demo-index report artifact pointers for the issue channel.

Inspect deterministic GitHub bridge state/report output:

```bash
cargo run -p tau-coding-agent -- \
  --github-status-inspect owner/repo \
  --github-state-dir .tau/github-issues \
  --github-status-json
```

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
  --multi-channel-fixture crates/tau-multi-channel/testdata/multi-channel-contract/baseline-three-channel.json \
  --multi-channel-state-dir .tau/multi-channel \
  --multi-channel-queue-limit 64 \
  --multi-channel-processed-event-cap 10000 \
  --multi-channel-retry-max-attempts 4 \
  --multi-channel-retry-base-delay-ms 0 \
  --multi-channel-media-understanding true \
  --multi-channel-media-max-attachments 4 \
  --multi-channel-media-max-summary-chars 280
```

The runner writes channel-store output under:

- `.tau/multi-channel/channel-store/telegram/...`
- `.tau/multi-channel/channel-store/discord/...`
- `.tau/multi-channel/channel-store/whatsapp/...`

Runtime state for duplicate suppression is persisted at:

- `.tau/multi-channel/state.json`
- `.tau/multi-channel/runtime-events.jsonl` (per-cycle observability log)

Inbound `channel-store` logs also include per-event `media_understanding` diagnostics for
supported image/audio/video attachments (and explicit skip/failure reason codes for unsupported
media).

Inspect multi-channel transport health snapshot:

```bash
cargo run -p tau-coding-agent -- \
  --multi-channel-state-dir .tau/multi-channel \
  --transport-health-inspect multi-channel \
  --transport-health-json
```

Inspect multi-channel rollout guardrail/status report:

```bash
cargo run -p tau-coding-agent -- \
  --multi-channel-state-dir .tau/multi-channel \
  --multi-channel-status-inspect \
  --multi-channel-status-json
```

Operational rollout and rollback guidance: `docs/guides/multi-channel-ops.md`.

## Multi-channel live runner (Telegram, Discord, WhatsApp)

Use this deterministic live-ingress mode to process local adapter inbox files without external
provider calls.

Ingress directory layout:

- `.tau/multi-channel/live-ingress/telegram.ndjson`
- `.tau/multi-channel/live-ingress/discord.ndjson`
- `.tau/multi-channel/live-ingress/whatsapp.ndjson`

Each line is one normalized provider envelope JSON object.

Run readiness preflight before enabling live mode (fails closed on required gaps):

```bash
cargo run -p tau-coding-agent -- \
  --multi-channel-live-readiness-preflight
```

JSON output mode:

```bash
cargo run -p tau-coding-agent -- \
  --multi-channel-live-readiness-preflight \
  --multi-channel-live-readiness-json
```

Readiness checks cover:

- Credential store readability (`--credential-store`)
- Live ingress directory path + per-channel inbox files
- Channel prerequisites for Telegram/Discord/WhatsApp via env var or integration secret
  (`/integration-auth set <integration-id> <secret>`)

Required channel secrets:

- Telegram: `TAU_TELEGRAM_BOT_TOKEN` or integration id `telegram-bot-token`
- Discord: `TAU_DISCORD_BOT_TOKEN` or integration id `discord-bot-token`
- WhatsApp access token: `TAU_WHATSAPP_ACCESS_TOKEN` or integration id `whatsapp-access-token`
- WhatsApp phone number id: `TAU_WHATSAPP_PHONE_NUMBER_ID` or integration id `whatsapp-phone-number-id`

One-shot provider payload ingest (raw payload -> live ingress NDJSON):

```bash
cargo run -p tau-coding-agent -- \
  --multi-channel-live-ingest-file ./crates/tau-multi-channel/testdata/multi-channel-live-ingress/raw/telegram-update.json \
  --multi-channel-live-ingest-transport telegram \
  --multi-channel-live-ingest-provider telegram-bot-api \
  --multi-channel-live-ingest-dir .tau/multi-channel/live-ingress
```

Supported ingest transports:

- `telegram`
- `discord`
- `whatsapp`

The command validates payload shape and appends one normalized envelope line to:

- `<ingest-dir>/telegram.ndjson`
- `<ingest-dir>/discord.ndjson`
- `<ingest-dir>/whatsapp.ndjson`

```bash
cargo run -p tau-coding-agent -- \
  --model openai/gpt-4o-mini \
  --multi-channel-live-runner \
  --multi-channel-live-ingress-dir .tau/multi-channel/live-ingress \
  --multi-channel-state-dir .tau/multi-channel \
  --multi-channel-queue-limit 64 \
  --multi-channel-processed-event-cap 10000 \
  --multi-channel-retry-max-attempts 4 \
  --multi-channel-retry-base-delay-ms 0
```

The live runner writes to the same state and channel-store paths as contract mode:

- `.tau/multi-channel/state.json`
- `.tau/multi-channel/runtime-events.jsonl`
- `.tau/multi-channel/channel-store/<transport>/<channel>/...`

Policy controls for DM/group behavior and mention gating:

- `.tau/security/channel-policy.json` (`dmPolicy`, `allowFrom`, `groupPolicy`, `requireMention`)
- `.tau/security/allowlist.json` and `.tau/security/pairings.json` (actor access controls)

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

## Browser automation live runner

```bash
cargo run -p tau-coding-agent -- \
  --model openai/gpt-4o-mini \
  --browser-automation-live-runner \
  --browser-automation-live-fixture crates/tau-coding-agent/testdata/browser-automation-live/live-sequence.json \
  --browser-automation-playwright-cli playwright-cli \
  --browser-automation-state-dir .tau/browser-automation \
```

The live runner executes fixture cases through an external Playwright-compatible CLI and writes
state and observability output under:

- `.tau/browser-automation/state.json`
- `.tau/browser-automation/runtime-events.jsonl`

Inspect browser automation transport health snapshot:

```bash
cargo run -p tau-coding-agent -- \
  --browser-automation-state-dir .tau/browser-automation \
  --transport-health-inspect browser-automation \
  --transport-health-json
```

Run browser automation readiness preflight:

```bash
cargo run -p tau-coding-agent -- \
  --browser-automation-preflight
```

JSON output mode:

```bash
cargo run -p tau-coding-agent -- \
  --browser-automation-preflight \
  --browser-automation-preflight-json
```

Troubleshooting:

- `browser_automation.npx` not ready: install Node.js/npm and ensure `npx` is on `PATH`.
- `browser_automation.playwright_cli` missing: install `@playwright/mcp` or set `--browser-automation-playwright-cli` to a valid wrapper binary.
- `--browser-automation-contract-runner` has been removed; use the live-runner command shown above.

Demo command path:

- `./scripts/demo/browser-automation.sh`
- `./scripts/demo/all.sh --only browser-automation --fail-fast`

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

## Gateway OpenResponses + websocket control server

Run the authenticated gateway server for HTTP OpenResponses plus websocket control-plane methods.

```bash
cargo run -p tau-coding-agent -- \
  --model openai/gpt-4o-mini \
  --gateway-openresponses-server \
  --gateway-openresponses-bind 127.0.0.1:8787 \
  --gateway-openresponses-auth-mode token \
  --gateway-openresponses-auth-token dev-secret \
  --gateway-state-dir .tau/gateway \
  --gateway-openresponses-rate-limit-window-seconds 60 \
  --gateway-openresponses-rate-limit-max-requests 120
```

Server endpoints:

- `POST /v1/responses` (OpenResponses subset)
- `POST /gateway/auth/session` (only when `--gateway-openresponses-auth-mode=password-session`)
- `GET /gateway/status`
- `GET /gateway/ws` (websocket control protocol)
- `GET /webchat`

Websocket control methods (schema versions `0` and `1` accepted):

- `capabilities.request`
- `gateway.status.request`
- `session.status.request`
- `session.reset.request`
- `run.lifecycle.status.request`

Example websocket frame:

```json
{"schema_version":1,"request_id":"req-cap","kind":"capabilities.request","payload":{}}
```

The server returns deterministic response envelopes and `error` frames for malformed or unsupported input. It also emits heartbeat signals (`ws.ping` plus `gateway.heartbeat`) on a fixed interval.

Compatibility fixtures for protocol replay:

- `crates/tau-coding-agent/testdata/gateway-ws-protocol/dispatch-supported-controls.json`
- `crates/tau-coding-agent/testdata/gateway-ws-protocol/dispatch-unsupported-schema-continues.json`
- `crates/tau-coding-agent/testdata/gateway-ws-protocol/dispatch-unknown-kind-continues.json`

## Tau daemon lifecycle

Use daemon lifecycle commands to install/start/stop/status/uninstall Tau daemon state and profile files.

Install profile files (auto resolves host profile):

```bash
cargo run -p tau-coding-agent -- \
  --daemon-install \
  --daemon-state-dir .tau/daemon \
  --daemon-profile auto
```

Start and stop lifecycle state:

```bash
cargo run -p tau-coding-agent -- \
  --daemon-start \
  --daemon-state-dir .tau/daemon

cargo run -p tau-coding-agent -- \
  --daemon-stop \
  --daemon-stop-reason maintenance_window \
  --daemon-state-dir .tau/daemon
```

Inspect status and diagnostics:

```bash
cargo run -p tau-coding-agent -- \
  --daemon-status \
  --daemon-status-json \
  --daemon-state-dir .tau/daemon
```

Uninstall profile files:

```bash
cargo run -p tau-coding-agent -- \
  --daemon-uninstall \
  --daemon-state-dir .tau/daemon
```

Subcommand alias is also supported:

```bash
cargo run -p tau-coding-agent -- daemon status --json --state-dir .tau/daemon
```

Generated profile files:

- launchd: `.tau/daemon/launchd/io.tau.coding-agent.plist`
- systemd user: `.tau/daemon/systemd/tau-coding-agent.service`

Runbook and troubleshooting commands: `docs/guides/daemon-ops.md`.

## Gateway contract runner

Use this fixture-driven runtime mode to validate Tau gateway request handling, retry outcomes,
state persistence, and channel-store snapshots.

```bash
cargo run -p tau-coding-agent -- \
  --model openai/gpt-4o-mini \
  --gateway-contract-runner \
  --gateway-fixture crates/tau-gateway/testdata/gateway-contract/rollout-pass.json \
  --gateway-state-dir .tau/gateway \
  --gateway-guardrail-failure-streak-threshold 2 \
  --gateway-guardrail-retryable-failures-threshold 2
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

## Deployment contract runner (cloud + WASM)

Use this fixture-driven runtime mode to validate cloud deployment and WASM rollout paths,
retry outcomes, state persistence, and channel-store snapshots.

```bash
cargo run -p tau-coding-agent -- \
  --model openai/gpt-4o-mini \
  --deployment-contract-runner \
  --deployment-fixture crates/tau-coding-agent/testdata/deployment-contract/rollout-pass.json \
  --deployment-state-dir .tau/deployment \
  --deployment-queue-limit 64 \
  --deployment-processed-case-cap 10000 \
  --deployment-retry-max-attempts 4 \
  --deployment-retry-base-delay-ms 0
```

The runner writes state and observability output under:

- `.tau/deployment/state.json`
- `.tau/deployment/runtime-events.jsonl`
- `.tau/deployment/channel-store/deployment/<blueprint_id>/...`

Inspect deployment transport health snapshot:

```bash
cargo run -p tau-coding-agent -- \
  --deployment-state-dir .tau/deployment \
  --transport-health-inspect deployment \
  --transport-health-json
```

Inspect deployment rollout guardrail/status report:

```bash
cargo run -p tau-coding-agent -- \
  --deployment-state-dir .tau/deployment \
  --deployment-status-inspect \
  --deployment-status-json
```

Operational rollout and rollback guidance: `docs/guides/deployment-ops.md`.

## No-code custom command contract runner

Use this fixture-driven runtime mode to validate no-code command registry lifecycle behavior,
retry outcomes, state persistence, and channel-store snapshots.

```bash
cargo run -p tau-coding-agent -- \
  --model openai/gpt-4o-mini \
  --custom-command-contract-runner \
  --custom-command-fixture crates/tau-coding-agent/testdata/custom-command-contract/rollout-pass.json \
  --custom-command-state-dir .tau/custom-command \
  --custom-command-queue-limit 64 \
  --custom-command-processed-case-cap 10000 \
  --custom-command-retry-max-attempts 4 \
  --custom-command-retry-base-delay-ms 0
```

The runner writes state and observability output under:

- `.tau/custom-command/state.json`
- `.tau/custom-command/runtime-events.jsonl`
- `.tau/custom-command/channel-store/custom-command/<command_name or registry>/...`

Inspect custom-command transport health snapshot:

```bash
cargo run -p tau-coding-agent -- \
  --custom-command-state-dir .tau/custom-command \
  --transport-health-inspect custom-command \
  --transport-health-json
```

Inspect custom-command rollout guardrail/status report:

```bash
cargo run -p tau-coding-agent -- \
  --custom-command-state-dir .tau/custom-command \
  --custom-command-status-inspect \
  --custom-command-status-json
```

Operational rollout and rollback guidance: `docs/guides/custom-command-ops.md`.

## Voice interaction and wake-word contract runner

Use this fixture-driven runtime mode to validate voice wake-word detection, turn handling,
retry outcomes, state persistence, and channel-store snapshots.

```bash
cargo run -p tau-coding-agent -- \
  --model openai/gpt-4o-mini \
  --voice-contract-runner \
  --voice-fixture crates/tau-coding-agent/testdata/voice-contract/rollout-pass.json \
  --voice-state-dir .tau/voice \
  --voice-queue-limit 64 \
  --voice-processed-case-cap 10000 \
  --voice-retry-max-attempts 4 \
  --voice-retry-base-delay-ms 0
```

The runner writes state and observability output under:

- `.tau/voice/state.json`
- `.tau/voice/runtime-events.jsonl`
- `.tau/voice/channel-store/voice/<speaker_id>/...`

Inspect voice transport health snapshot:

```bash
cargo run -p tau-coding-agent -- \
  --voice-state-dir .tau/voice \
  --transport-health-inspect voice \
  --transport-health-json
```

Inspect voice rollout guardrail/status report:

```bash
cargo run -p tau-coding-agent -- \
  --voice-state-dir .tau/voice \
  --voice-status-inspect \
  --voice-status-json
```

Operational rollout and rollback guidance: `docs/guides/voice-ops.md`.

## Voice live session replay runner

Use this fixture-driven live mode to validate wake-word routing, live turn handling, and
fallback behavior (invalid audio/provider outages).

```bash
cargo run -p tau-coding-agent -- \
  --model openai/gpt-4o-mini \
  --voice-live-runner \
  --voice-live-input crates/tau-coding-agent/testdata/voice-live/single-turn.json \
  --voice-live-wake-word tau \
  --voice-live-max-turns 64 \
  --voice-live-tts-output \
  --voice-state-dir .tau/voice-live
```

The runner writes state and observability output under:

- `.tau/voice-live/state.json`
- `.tau/voice-live/runtime-events.jsonl`
- `.tau/voice-live/channel-store/voice/<speaker_id>/...`

Inspect live voice transport health snapshot:

```bash
cargo run -p tau-coding-agent -- \
  --voice-state-dir .tau/voice-live \
  --transport-health-inspect voice \
  --transport-health-json
```

Inspect live voice rollout guardrail/status report:

```bash
cargo run -p tau-coding-agent -- \
  --voice-state-dir .tau/voice-live \
  --voice-status-inspect \
  --voice-status-json
```

Operational rollout and rollback guidance: `docs/guides/voice-ops.md`.

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
