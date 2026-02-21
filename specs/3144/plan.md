# Plan: Issue #3144 - /ops/config profile and policy control contracts

## Approach
1. Add RED tests for missing profile/policy control markers in `tau-dashboard-ui` and gateway route integration.
2. Extend `/ops/config` panel with deterministic profile and policy control sections.
3. Keep panel visibility behavior unchanged for non-config routes.
4. Run scoped regression tests (`spec_3140`) plus fmt/clippy.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-dashboard-ui/src/tests.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks & Mitigations
- Risk: adding dense config markup can accidentally break existing panel routing visibility.
  - Mitigation: regression case C-04 + rerun `spec_3140`.
- Risk: markup drift between UI and gateway route rendering.
  - Mitigation: gateway integration conformance test for `/ops/config`.

## Interfaces / Contracts
New deterministic markers:
- `#tau-ops-config-profile-controls`
- `#tau-ops-config-policy-controls`

## ADR
No ADR required.
