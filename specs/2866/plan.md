# Plan: Issue #2866 - chat inline tool-result card contracts

## Approach
1. Add deterministic inline tool-card marker rendering in `tau-dashboard-ui` transcript rows for `role == "tool"`.
2. Add UI and gateway tests covering tool and non-tool row behavior across `/ops`, `/ops/chat`, and `/ops/sessions`.
3. Re-run required chat/panel regression suites and full verification gates.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations
- Risk: introducing markup changes that break existing chat row assertions.
  - Mitigation: keep existing row IDs/attributes stable and add card marker as additive nested element.
- Risk: route-hidden chat panel behavior regression.
  - Mitigation: explicit integration assertions for `/ops` and `/ops/sessions` with tool sessions.

## Interface / Contract Notes
- Additive SSR marker behavior only.
- No transport/API/schema changes.
