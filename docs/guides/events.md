# Events Guide

Run all commands from repository root.

## Validate and inspect event definitions

Validate definitions:

```bash
cargo run -p tau-coding-agent -- \
  --events-dir ./examples/events \
  --events-state-path ./examples/events-state.json \
  --events-validate \
  --events-validate-json
```

Inspect queue posture:

```bash
cargo run -p tau-coding-agent -- \
  --events-dir ./examples/events \
  --events-state-path ./examples/events-state.json \
  --events-inspect \
  --events-inspect-json
```

Simulate horizon posture:

```bash
cargo run -p tau-coding-agent -- \
  --events-dir ./examples/events \
  --events-state-path ./examples/events-state.json \
  --events-simulate \
  --events-simulate-horizon-seconds 3600 \
  --events-simulate-json
```

Dry-run execution selection:

```bash
cargo run -p tau-coding-agent -- \
  --events-dir ./examples/events \
  --events-state-path ./examples/events-state.json \
  --events-queue-limit 64 \
  --events-dry-run \
  --events-dry-run-json
```

Strict dry-run gating for CI:

```bash
cargo run -p tau-coding-agent -- \
  --events-dir ./examples/events \
  --events-state-path ./examples/events-state.json \
  --events-dry-run \
  --events-dry-run-strict
```

## Run the scheduler loop

```bash
cargo run -p tau-coding-agent -- \
  --model openai/gpt-4o-mini \
  --events-runner \
  --events-dir .tau/events \
  --events-state-path .tau/events/state.json \
  --events-poll-interval-ms 1000 \
  --events-queue-limit 64
```

## Write event templates

```bash
cargo run -p tau-coding-agent -- \
  --events-template-write .tau/events/daily-status.json \
  --events-template-schedule periodic \
  --events-template-channel github/owner/repo#42 \
  --events-template-id daily-status \
  --events-template-cron "0 0/15 * * * * *" \
  --events-template-timezone UTC
```

## Webhook ingest commands

Debounced immediate-event ingest:

```bash
cargo run -p tau-coding-agent -- \
  --events-dir .tau/events \
  --events-state-path .tau/events/state.json \
  --event-webhook-ingest-file /tmp/webhook.json \
  --event-webhook-channel slack/C123 \
  --event-webhook-prompt-prefix "Handle incoming deployment signal." \
  --event-webhook-debounce-key deploy-hook \
  --event-webhook-debounce-window-seconds 60
```

Signed payload verification mode:

```bash
cargo run -p tau-coding-agent -- \
  --events-dir .tau/events \
  --events-state-path .tau/events/state.json \
  --event-webhook-ingest-file /tmp/webhook.json \
  --event-webhook-channel github/owner/repo#42 \
  --event-webhook-signature "$X_HUB_SIGNATURE_256" \
  --event-webhook-secret "$WEBHOOK_SECRET" \
  --event-webhook-signature-algorithm github-sha256 \
  --event-webhook-signature-max-skew-seconds 300
```

## Demo scripts

```bash
./scripts/demo/events.sh
./scripts/demo-smoke.sh
```
