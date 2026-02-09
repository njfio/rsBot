# Multi-Channel Contract Fixtures

This directory contains deterministic fixtures for the normalized inbound event contract used by
the Telegram, Discord, and WhatsApp transport roadmap work.

- `baseline-three-channel.json`: valid fixture with one event per supported channel.
- `duplicate-event-key.json`: invalid fixture that intentionally duplicates a transport/event key.
- `invalid-attachment-url.json`: invalid fixture that intentionally uses a disallowed URL scheme.

Schema:

- Top-level `schema_version` must match `MULTI_CHANNEL_CONTRACT_SCHEMA_VERSION`.
- Each event includes `transport`, `event_kind`, identity fields, timestamp, and message content.
- Attachments must use `https://` URLs (or `http://localhost` for local test hooks).
