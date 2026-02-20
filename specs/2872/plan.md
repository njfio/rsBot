# Plan: Issue #2872 - chat new-session creation contracts

## Approach
1. Add additive chat new-session form markers in `tau-dashboard-ui` chat panel.
2. Add `POST /ops/chat/new` gateway handler that initializes target session and redirects to chat route preserving theme/sidebar query state.
3. Add UI and gateway conformance tests for create+redirect+selector+hidden-route panel contracts.
4. Re-run required chat regressions and verification gates.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations
- Risk: route/query-state regressions across chat/sessions views.
  - Mitigation: integration tests for `/ops/chat`, `/ops`, and `/ops/sessions` after new-session creation.
- Risk: session selector order assumptions in tests.
  - Mitigation: assert presence/selection markers, not fragile ordering beyond deterministic contract expectations.

## Interface / Contract Notes
- Additive route `POST /ops/chat/new` (internal ops shell endpoint).
- No schema/protocol changes outside ops shell route handling.
