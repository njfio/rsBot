# Plan: Issue #3220 - move gateway compat/telemetry runtime state into module

## Approach
1. RED: tighten size guard threshold to `1450` and require compat-state module wiring.
2. Extract compat/telemetry runtime-state structs/enums + `GatewayOpenResponsesServerState` helper methods into new `compat_state_runtime.rs`.
3. Re-run size guard, targeted compat/telemetry integration tests, and quality gates.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/compat_state_runtime.rs` (new)
- `scripts/dev/test-gateway-openresponses-size.sh`
- `specs/milestones/m232/index.md`
- `specs/3220/spec.md`
- `specs/3220/plan.md`
- `specs/3220/tasks.md`

## Risks & Mitigations
- Risk: status counter drift for compat/telemetry.
  - Mitigation: targeted integration tests for both counter surfaces.
- Risk: type visibility mismatch across modules.
  - Mitigation: use `pub(super)` visibility and `super::*` imports consistently.

## Interfaces / Contracts
- `/gateway/status` payload fields for compat/telemetry unchanged.
- No endpoint path changes.

## ADR
No ADR required (internal module extraction only).
