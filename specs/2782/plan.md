# Plan: Issue #2782 - PRD Phase 1A Leptos crate and /ops shell integration

## Approach
1. Capture RED evidence for missing crate/route contracts.
2. Add `tau-dashboard-ui` crate with Leptos SSR render function and baseline shell components.
3. Wire crate into workspace and gateway dependencies.
4. Add `/ops` route in gateway router and corresponding endpoint test.
5. Run scoped fmt/clippy/test verification.

## Affected Modules
- `Cargo.toml`
- `Cargo.lock`
- `crates/tau-dashboard-ui/**` (new)
- `crates/tau-gateway/Cargo.toml`
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`
- `specs/2782/*`

## Risks and Mitigations
- Risk: Leptos dependency footprint impacts compile times.
  - Mitigation: constrain to SSR-only minimal feature set in this slice.
- Risk: gateway route drift or existing shell regressions.
  - Mitigation: add isolated `/ops` route and keep `/dashboard` route unchanged.

## Interface and Contract Notes
- New public function in `tau-dashboard-ui`: SSR shell render function.
- New gateway endpoint constant and route: `/ops`.
