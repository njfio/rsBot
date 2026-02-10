# Gateway Websocket Protocol Compatibility Fixtures

This fixture corpus locks deterministic websocket control-frame behavior for Tau gateway schema compatibility and fail-closed handling.

- `dispatch-supported-controls.json`: supported control methods for capabilities, gateway status, session status/reset, and run lifecycle status.
- `dispatch-unsupported-schema-continues.json`: unsupported request schema regression while processing continues for later valid frames.
- `dispatch-unknown-kind-continues.json`: unsupported method regression while processing continues for later valid frames.

Each fixture includes ordered input frames, expected processed/error counts, and exact response envelopes.
