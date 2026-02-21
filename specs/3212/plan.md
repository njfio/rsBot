# Plan: Issue #3212 - extract gateway events status collector with contract parity

## Approach
1. Add RED guard script asserting root gateway module size threshold and extracted module presence.
2. Move events status structs + collector function from `gateway_openresponses.rs` into new `events_status.rs`.
3. Rewire imports/calls and run status integration tests plus quality gates.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/events_status.rs` (new)
- `scripts/dev/test-gateway-openresponses-size.sh` (new)
- `specs/milestones/m230/index.md`
- `specs/3212/spec.md`
- `specs/3212/plan.md`
- `specs/3212/tasks.md`

## Risks & Mitigations
- Risk: accidental status payload drift during refactor.
  - Mitigation: targeted integration tests for configured/unconfigured events status responses.
- Risk: brittle size guard threshold.
  - Mitigation: set threshold above immediate post-split footprint while still enforcing meaningful reduction.

## Interfaces / Contracts
- `GET /gateway/status` JSON structure remains unchanged for `events` section.
- Route and auth behavior unchanged.

## ADR
No ADR required (internal module extraction and test guard only).
