# Inbound Safety Corpus

Deterministic inbound prompt-injection fixtures used by runtime safety tests.

- `transport-sourced-prompt-injection.json`: malicious and benign inbound payload cases
  across GitHub Issues, Slack, and multi-channel ingress variants.

Contract notes:
- `schema_version` is currently `1`.
- Each case defines `transport`, `payload`, and `malicious`.
- Malicious cases include `expected_reason_code` used by fail-closed block-mode assertions.
