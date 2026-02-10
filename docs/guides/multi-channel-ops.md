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
- `.tau/multi-channel/runtime-events.jsonl`
- `.tau/multi-channel/route-traces.jsonl`
- `.tau/multi-channel/security/multi-channel-route-bindings.json`
- `.tau/multi-channel/channel-store/<transport>/<channel>/...`

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
2. Run deterministic demo (contract + live ingress path):
   `./scripts/demo/multi-channel.sh`
3. Confirm health snapshot is `healthy` before promotion:
   `--transport-health-inspect multi-channel --transport-health-json`
4. Confirm status rollout gate is `pass` before promotion:
   `--multi-channel-status-inspect --multi-channel-status-json`
5. Promote by increasing fixture/event complexity incrementally while monitoring:
   `failure_streak`, `last_cycle_failed`, `queue_depth`.

## Rollback plan

1. Stop invoking `--multi-channel-contract-runner` and `--multi-channel-live-runner`.
2. Preserve `.tau/multi-channel/` for incident analysis.
3. Revert to last known-good revision:
   `git revert <commit>`
4. Re-run validation matrix before re-enable.

## Troubleshooting

- Symptom: health state `degraded` with `event_processing_failed`.
  Action: inspect `runtime-events.jsonl`, then inspect channel-store write paths and filesystem permissions.
- Symptom: health state `degraded` with `retry_attempted`.
  Action: inspect per-event metadata for `simulate_transient_failures` and increase retry settings only when needed.
- Symptom: health state `failing` (`failure_streak >= 3`).
  Action: treat as rollout gate failure; pause promotion and investigate repeated processing failures.
- Symptom: non-zero `queue_depth`.
  Action: increase `--multi-channel-queue-limit` or reduce fixture batch size to avoid backpressure drops.
