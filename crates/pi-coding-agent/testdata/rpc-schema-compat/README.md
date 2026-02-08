# RPC Schema Compatibility Fixtures

This fixture corpus locks deterministic RPC behavior across request schema versions and serve/dispatch modes.

- `dispatch-mixed-supported.json`: supported schema versions (`0`, `1`) in preflight dispatch mode.
- `dispatch-unsupported-continues.json`: unsupported schema regression while continuing to later valid frames.
- `serve-mixed-supported.json`: supported schema versions (`0`, `1`) in stateful serve mode.
- `serve-cancel-supported.json`: serve-mode cancel lifecycle with deterministic terminal stream events.
- `serve-unsupported-continues.json`: unsupported schema regression while serve mode continues processing.

Each fixture file includes input lines, expected processing/error counts, and expected response envelopes.
Terminal fixture expectations assert explicit `terminal` and `terminal_state` metadata for terminal lifecycle envelopes/events.
Terminal assistant stream expectations assert `final: true` plus deterministic summary deltas on closure transitions.
Serve-mode fixtures also lock `run.status` semantics for closed known runs (`known: true` with terminal metadata).
Status fixtures lock explicit `terminal` flag behavior across active, inactive/unknown, and closed states.
Closed-run status retention is intentionally bounded in serve mode; these fixtures cover behavior within that deterministic retention window.
