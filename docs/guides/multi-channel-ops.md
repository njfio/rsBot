# Multi-channel Operations Runbook

Run all commands from repository root.

## Scope

This runbook covers Multi-channel runtime operations for:

- Telegram
- Discord
- WhatsApp

## Health and observability signals

Primary transport-health signal:

```bash
cargo run -p tau-coding-agent -- \
  --multi-channel-state-dir .tau/multi-channel \
  --transport-health-inspect multi-channel \
  --transport-health-json
```

Primary rollout/status signal:

```bash
cargo run -p tau-coding-agent -- \
  --multi-channel-state-dir .tau/multi-channel \
  --multi-channel-status-inspect \
  --multi-channel-status-json
```

Primary state files:

- `.tau/multi-channel/state.json`
- `.tau/multi-channel/live-connectors-state.json`
- `.tau/multi-channel/runtime-events.jsonl`
- `.tau/multi-channel/route-traces.jsonl`
- `.tau/multi-channel/security/channel-lifecycle.json`
- `.tau/multi-channel/security/multi-channel-route-bindings.json`
- `.tau/multi-channel/channel-store/<transport>/<channel>/...`

Telemetry controls (runtime flags, privacy-safe defaults):

- `--multi-channel-telemetry-typing-presence=true|false`
- `--multi-channel-telemetry-usage-summary=true|false`
- `--multi-channel-telemetry-include-identifiers=true|false` (default `false`)
- `--multi-channel-telemetry-min-response-chars=<N>` (default `120`)

Media understanding controls (runtime flags, bounded defaults):

- `--multi-channel-media-understanding=true|false` (default `true`)
- `--multi-channel-media-max-attachments=<N>` (default `4`)
- `--multi-channel-media-max-summary-chars=<N>` (default `280`)

Inbound channel-store log entries include a `media_understanding` payload with per-attachment
decision and reason-code details (`processed|skipped|failed`).

`--multi-channel-status-inspect` now includes telemetry counters and policy snapshot:

- lifecycle counters: typing + presence totals/per-transport
- usage counters: records/chars/chunks/cost totals/per-transport
- active telemetry policy toggles and threshold

`runtime-events.jsonl` reason codes:

- `healthy_cycle`
- `queue_backpressure_applied`
- `duplicate_events_skipped`
- `retry_attempted`
- `transient_failures_observed`
- `event_processing_failed`
- `pairing_policy_permissive`
- `pairing_policy_enforced`
- `pairing_policy_denied_events`
- `telemetry_lifecycle_emitted`
- `telemetry_usage_summary_emitted`

Attachment-level media-understanding reason codes are persisted per event in channel-store logs:

- `media_image_described`
- `media_audio_transcribed`
- `media_video_summarized`
- `media_unsupported_attachment_type`
- `media_attachment_limit_exceeded`
- `media_duplicate_attachment`
- `media_provider_error`

## Pairing and allowlist policy

The multi-channel runtime evaluates pairing/allowlist policy per inbound event before generating a
response.

Policy files:

- `.tau/security/allowlist.json`
- `.tau/security/pairings.json`

Policy channel key format used by Telegram/Discord/WhatsApp:

- `<transport>:<conversation_id>`

Examples:

- `telegram:chat-100`
- `discord:discord-channel-88`
- `whatsapp:phone-55:15551234567`

When strict policy is active, unknown actors are denied with deterministic reason codes
(`deny_actor_not_paired_or_allowlisted`, `deny_actor_id_missing`). Denied events are recorded in
channel-store logs and runtime cycle reason codes.

## Channel policy model

Tau supports channel policy controls at:

- `.tau/security/channel-policy.json`

Schema fields (per default policy and per-channel overrides):

- `dmPolicy`: `allow` | `deny`
- `allowFrom`: `any` | `allowlist_or_pairing` | `allowlist_only`
- `groupPolicy`: `allow` | `deny`
- `requireMention`: `true` | `false`

Minimal policy file:

```json
{
  "schema_version": 1,
  "strictMode": false,
  "defaultPolicy": {
    "dmPolicy": "allow",
    "allowFrom": "allowlist_or_pairing",
    "groupPolicy": "allow",
    "requireMention": false
  },
  "channels": {
    "discord:ops-room": {
      "dmPolicy": "allow",
      "allowFrom": "any",
      "groupPolicy": "allow",
      "requireMention": true
    }
  }
}
```

Secure-default migration notes:

1. Keep `allowFrom` on `allowlist_or_pairing` while onboarding channels.
2. Set explicit `groupPolicy`/`requireMention` for high-traffic shared channels.
3. Avoid `dmPolicy=allow` with `allowFrom=any` in production unless intentionally open.
4. Enable `strictMode=true` to make unsafe open-DM combinations fail readiness preflight.

## Route bindings and multi-agent routing

Tau can bind ingress events to deterministic multi-agent route decisions using selectors for:

- `transport`
- `account_id`
- `conversation_id`
- `actor_id`

Binding file path:

- `.tau/multi-channel/security/multi-channel-route-bindings.json`

Minimal example:

```json
{
  "schema_version": 1,
  "bindings": [
    {
      "binding_id": "discord-ops",
      "transport": "discord",
      "account_id": "discord-main",
      "conversation_id": "ops-room",
      "actor_id": "*",
      "phase": "delegated_step",
      "category_hint": "incident",
      "session_key_template": "session-{role}"
    }
  ]
}
```

Deterministic fallback behavior when no binding matches:

- route selection uses default phase mapping (`command->planner`, `system->review`, others delegated)
- session key defaults to normalized `conversation_id`

Route trace output:

- every processed event appends one line to `.tau/multi-channel/route-traces.jsonl`
- channel-store inbound/outbound logs include a `route` payload and `route_session_key`

Inspect route evaluation for one event file:

```bash
cargo run -p tau-coding-agent -- \
  --multi-channel-state-dir .tau/multi-channel \
  --multi-channel-route-inspect-file ./crates/tau-coding-agent/testdata/multi-channel-live-ingress/telegram-valid.json \
  --multi-channel-route-inspect-json
```

## Deterministic demo path

```bash
./scripts/demo/multi-channel.sh
```

## Live ingress directory contract

`--multi-channel-live-runner` consumes local inbox files:

- `.tau/multi-channel/live-ingress/telegram.ndjson`
- `.tau/multi-channel/live-ingress/discord.ndjson`
- `.tau/multi-channel/live-ingress/whatsapp.ndjson`

Each line must be one normalized provider envelope JSON object. Invalid lines are skipped with
explicit parse diagnostics in stderr; valid lines continue processing.

One-shot raw provider ingest command (appends normalized envelopes to live-ingress files):

```bash
cargo run -p tau-coding-agent -- \
  --multi-channel-live-ingest-file ./crates/tau-coding-agent/testdata/multi-channel-live-ingress/raw/telegram-update.json \
  --multi-channel-live-ingest-transport telegram \
  --multi-channel-live-ingest-provider telegram-bot-api \
  --multi-channel-live-ingest-dir .tau/multi-channel/live-ingress
```

Use the same command for `discord` and `whatsapp` payloads by changing
`--multi-channel-live-ingest-transport` and `--multi-channel-live-ingest-file`.

## Live connector runner (polling + webhook)

`--multi-channel-live-connectors-runner` bridges provider APIs directly into
`--multi-channel-live-ingest-dir` NDJSON inbox files, then existing
`--multi-channel-live-runner` processes those envelopes.

Connector mode support:

- Telegram: `disabled`, `polling`, `webhook`
- Discord: `disabled`, `polling`
- WhatsApp: `disabled`, `webhook`

Polling example (one cycle, deterministic exit):

```bash
cargo run -p tau-coding-agent -- \
  --multi-channel-live-connectors-runner \
  --multi-channel-live-connectors-poll-once \
  --multi-channel-live-ingest-dir .tau/multi-channel/live-ingress \
  --multi-channel-live-connectors-state-path .tau/multi-channel/live-connectors-state.json \
  --multi-channel-telegram-ingress-mode polling \
  --multi-channel-discord-ingress-mode polling \
  --multi-channel-discord-ingress-channel-id discord-room-1,discord-room-2 \
  --multi-channel-telegram-bot-token <token> \
  --multi-channel-discord-bot-token <token>
```

Webhook example (long-running bridge):

```bash
cargo run -p tau-coding-agent -- \
  --multi-channel-live-connectors-runner \
  --multi-channel-live-ingest-dir .tau/multi-channel/live-ingress \
  --multi-channel-live-connectors-state-path .tau/multi-channel/live-connectors-state.json \
  --multi-channel-live-webhook-bind 127.0.0.1:8788 \
  --multi-channel-telegram-ingress-mode webhook \
  --multi-channel-whatsapp-ingress-mode webhook \
  --multi-channel-telegram-webhook-secret <secret> \
  --multi-channel-whatsapp-webhook-verify-token <verify-token> \
  --multi-channel-whatsapp-webhook-app-secret <app-secret>
```

Status inspect:

```bash
cargo run -p tau-coding-agent -- \
  --multi-channel-live-connectors-status \
  --multi-channel-live-connectors-status-json \
  --multi-channel-live-connectors-state-path .tau/multi-channel/live-connectors-state.json
```

Connector status fields include:

- per-channel `liveness`, `events_ingested`, `duplicates_skipped`
- `retry_attempts`, `auth_failures`, `parse_failures`, `provider_failures`
- `last_error_code`, `last_success_unix_ms`, `last_error_unix_ms`

Secret resolution follows existing outbound secret flow:

- direct CLI/env (`--multi-channel-telegram-bot-token`, etc.)
- integration store IDs (`telegram-bot-token`, `discord-bot-token`, `whatsapp-access-token`)

## Channel lifecycle operations

Tau supports deterministic lifecycle operations per transport:

- `status`: read persisted lifecycle state and readiness.
- `login`: initialize lifecycle state and create the transport ingress file.
- `logout`: persist logged-out lifecycle state.
- `probe`: evaluate readiness and persist probe result.

Commands:

```bash
# login/init
cargo run -p tau-coding-agent -- \
  --multi-channel-state-dir .tau/multi-channel \
  --multi-channel-live-ingress-dir .tau/multi-channel/live-ingress \
  --multi-channel-channel-login telegram \
  --multi-channel-telegram-bot-token <token> \
  --multi-channel-channel-login-json

# status
cargo run -p tau-coding-agent -- \
  --multi-channel-state-dir .tau/multi-channel \
  --multi-channel-live-ingress-dir .tau/multi-channel/live-ingress \
  --multi-channel-channel-status telegram \
  --multi-channel-telegram-bot-token <token> \
  --multi-channel-channel-status-json

# probe
cargo run -p tau-coding-agent -- \
  --multi-channel-state-dir .tau/multi-channel \
  --multi-channel-live-ingress-dir .tau/multi-channel/live-ingress \
  --multi-channel-channel-probe telegram \
  --multi-channel-telegram-bot-token <token> \
  --multi-channel-channel-probe-json

# logout/reset
cargo run -p tau-coding-agent -- \
  --multi-channel-state-dir .tau/multi-channel \
  --multi-channel-live-ingress-dir .tau/multi-channel/live-ingress \
  --multi-channel-channel-logout telegram \
  --multi-channel-channel-logout-json
```

Lifecycle reason codes include:

- `ready`
- `missing_telegram_bot_token`
- `missing_discord_bot_token`
- `missing_whatsapp_access_token`
- `missing_whatsapp_phone_number_id`
- `ingress_missing`
- `ingress_not_file`
- `credential_store_unreadable`
- `logout_requested`

## Native `/tau` commands in channel messages

Multi-channel runtime now intercepts `/tau ...` messages and executes a bounded operator command
surface directly from inbound Telegram/Discord/WhatsApp events.

Supported commands:

- `/tau help`
- `/tau status`
- `/tau auth status [openai|anthropic|google]`
- `/tau doctor [--online]`

Command responses include a normalized footer:

- `Tau command /tau <command> | status <reported|failed> | reason_code <...>`

Command metadata is persisted in outbound channel-store log payloads under:

- `command.schema` = `multi_channel_tau_command_v1`
- `command.command`
- `command.status`
- `command.reason_code`

Operator scope rule:

- `/tau auth status` and `/tau doctor` require allowlisted operator scope
  (`allow_allowlist` or `allow_allowlist_and_pairing` pairing outcomes).
- When scope is insufficient, command execution fails closed with
  `command_rbac_denied`.

## Outbound delivery modes

Multi-channel runtime supports outbound modes:

- `channel-store`: log outbound response only (no provider adapter dispatch)
- `dry-run`: shape provider requests and emit deterministic delivery receipts without network calls
- `provider`: dispatch outbound responses through provider HTTP adapters

Recommended deterministic CI mode:

```bash
cargo run -p tau-coding-agent -- \
  --multi-channel-contract-runner \
  --multi-channel-fixture ./crates/tau-coding-agent/testdata/multi-channel-contract/baseline-three-channel.json \
  --multi-channel-state-dir .tau/multi-channel \
  --multi-channel-outbound-mode dry-run \
  --multi-channel-outbound-max-chars 512
```

Provider mode example:

```bash
cargo run -p tau-coding-agent -- \
  --multi-channel-live-runner \
  --multi-channel-live-ingress-dir .tau/multi-channel/live-ingress \
  --multi-channel-state-dir .tau/multi-channel \
  --multi-channel-outbound-mode provider \
  --multi-channel-outbound-max-chars 1200 \
  --multi-channel-outbound-http-timeout-ms 5000
```

Outbound delivery failure reason codes surfaced in channel-store outbound logs:

- `delivery_missing_telegram_bot_token`
- `delivery_missing_discord_bot_token`
- `delivery_missing_whatsapp_access_token`
- `delivery_missing_whatsapp_phone_number_id`
- `delivery_rate_limited`
- `delivery_provider_unavailable`
- `delivery_request_rejected`
- `delivery_transport_error`

## Live readiness preflight

Run this gate before enabling `--multi-channel-live-runner`:

```bash
cargo run -p tau-coding-agent -- \
  --multi-channel-live-readiness-preflight
```

Machine-readable output:

```bash
cargo run -p tau-coding-agent -- \
  --multi-channel-live-readiness-preflight \
  --multi-channel-live-readiness-json
```

Fail-closed behavior:

- Exit code is non-zero when required prerequisites fail
- Output includes deterministic reason codes (`key:code`) for remediation

Required channel prerequisites:

- Telegram: `TAU_TELEGRAM_BOT_TOKEN` or integration id `telegram-bot-token`
- Discord: `TAU_DISCORD_BOT_TOKEN` or integration id `discord-bot-token`
- WhatsApp token: `TAU_WHATSAPP_ACCESS_TOKEN` or integration id `whatsapp-access-token`
- WhatsApp phone id: `TAU_WHATSAPP_PHONE_NUMBER_ID` or integration id `whatsapp-phone-number-id`

Channel secrets can be seeded via integration store:

```bash
/integration-auth set telegram-bot-token <secret>
/integration-auth set discord-bot-token <secret>
/integration-auth set whatsapp-access-token <secret>
/integration-auth set whatsapp-phone-number-id <value>
```

## Rollout plan with guardrails

1. Validate fixture/live runtime locally:
   `cargo test -p tau-coding-agent multi_channel -- --nocapture`
2. Validate connector ingest paths:
   `cargo test -p tau-coding-agent multi_channel_live_connectors -- --test-threads=1`
3. Run deterministic demo (contract + live ingress path):
   `./scripts/demo/multi-channel.sh`
4. Confirm health snapshot is `healthy` before promotion:
   `--transport-health-inspect multi-channel --transport-health-json`
5. Confirm status rollout gate is `pass` before promotion:
   `--multi-channel-status-inspect --multi-channel-status-json`
6. For connector mode, confirm connector status liveness before promotion:
   `--multi-channel-live-connectors-status --multi-channel-live-connectors-status-json`
7. Promote by increasing fixture/event complexity incrementally while monitoring:
   `failure_streak`, `last_cycle_failed`, `queue_depth`.

## Rollback plan

1. Stop invoking `--multi-channel-live-connectors-runner`.
2. Stop invoking `--multi-channel-contract-runner` and `--multi-channel-live-runner`.
3. Preserve `.tau/multi-channel/` for incident analysis.
4. Revert to last known-good revision:
   `git revert <commit>`
5. Re-run validation matrix before re-enable.

## Troubleshooting

- Symptom: health state `degraded` with `event_processing_failed`.
  Action: inspect `runtime-events.jsonl`, then inspect channel-store write paths and filesystem permissions.
- Symptom: health state `degraded` with `retry_attempted`.
  Action: inspect per-event metadata for `simulate_transient_failures` and increase retry settings only when needed.
- Symptom: health state `failing` (`failure_streak >= 3`).
  Action: treat as rollout gate failure; pause promotion and investigate repeated processing failures.
- Symptom: non-zero `queue_depth`.
  Action: increase `--multi-channel-queue-limit` or reduce fixture batch size to avoid backpressure drops.
