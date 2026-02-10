# Multi-Channel Live Ingress Fixtures

These fixtures model normalized provider envelopes for live ingress parsing.

Supported transports:
- `telegram`
- `discord`
- `whatsapp`

Each envelope uses this top-level shape:

```json
{
  "schema_version": 1,
  "transport": "telegram|discord|whatsapp",
  "provider": "provider-name",
  "payload": { "... provider payload ..." }
}
```

Files:
- `telegram-valid.json`: valid Telegram envelope.
- `discord-valid.json`: valid Discord envelope.
- `whatsapp-valid.json`: valid WhatsApp envelope.
- `invalid-unsupported-transport.json`: unsupported transport regression sample.
- `invalid-discord-missing-author.json`: missing required `payload.author` regression sample.

Raw provider payload fixtures (for one-shot ingest command):
- `raw/telegram-update.json`
- `raw/discord-message.json`
- `raw/whatsapp-message.json`
