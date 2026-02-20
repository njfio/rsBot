# Plan: Issue #2943 - Real-time stream connection markers and conformance tests

## Approach
1. Add RED conformance tests in `crates/tau-dashboard-ui/src/tests.rs` for stream contract markers C-01..C-06.
2. Add a dedicated stream contract marker section in `render_tau_ops_dashboard_shell_with_context`.
3. Keep marker IDs and `data-*` tokens deterministic and stable for contract verification.
4. Re-run scoped verify gates (fmt, clippy, tests).

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-dashboard-ui/src/tests.rs`
- `specs/2943/spec.md`
- `specs/2943/tasks.md`

## Risks / Mitigations
- Risk: marker drift with existing test contracts.
  - Mitigation: use new isolated `tau-ops-stream-contract` section IDs without modifying existing IDs.
- Risk: accidental behavior coupling across routes.
  - Mitigation: stream marker section remains route-agnostic and declarative.

## Interfaces / Contracts
- No public API signature change.
- Add HTML contract markers:
  - stream transport/bootstrap
  - heartbeat/alerts/chat/connector targets
  - reconnect strategy/backoff attributes

## ADR
No ADR required: no new dependencies, protocol changes, or architecture boundaries introduced.
