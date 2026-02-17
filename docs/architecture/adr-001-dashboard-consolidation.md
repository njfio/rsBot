# ADR-001: Dashboard Runtime Consolidation

## Context

Tau has two dashboard-related surfaces:

- Gateway-served dashboard APIs and stream endpoints in `tau-gateway`.
- A `tau-dashboard` contract-runner crate used for fixture-oriented replay.

Operational dashboard behavior (health, widgets, queue timeline, alerts,
actions, stream, and auth checks) is already implemented and tested in
`tau-gateway`. Roadmap claim #8 remained partial because the consolidation
decision was not explicitly documented and validated through a single command.

## Decision

The production dashboard runtime surface is consolidated on `tau-gateway`.

- Runtime and operator-facing dashboard behavior is owned by gateway endpoints
  and dashboard state artifacts under `.tau/dashboard`.
- `tau-dashboard` remains a contract/fixture support surface and is not the
  production runtime path.
- Verification of the consolidated behavior is standardized through
  `scripts/dev/verify-dashboard-consolidation.sh`.

## Consequences

- Roadmap/status reporting should treat dashboard runtime capability as provided
  by gateway, validated via executable gateway/onboarding tests.
- Future standalone dashboard UI work is additive and does not redefine runtime
  ownership away from gateway without a superseding ADR.
- The dashboard contract-runner pathway remains useful for fixtures and replay,
  but not as the deployment control plane.
