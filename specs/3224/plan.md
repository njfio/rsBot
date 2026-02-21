# Plan: Issue #3224 - move gateway multi-channel status/report types into module

## Approach
1. RED: tighten root size guard to `1300` and add guard that multi-channel status type definitions are absent from root.
2. Move multi-channel status/report structs + defaults from root into `multi_channel_status.rs`.
3. Move multi-channel state/event DTOs from root into `multi_channel_status.rs`.
4. Verify size guard, targeted tests, and quality gates.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/multi_channel_status.rs`
- `scripts/dev/test-gateway-openresponses-size.sh`
- `specs/milestones/m233/index.md`
- `specs/3224/spec.md`
- `specs/3224/plan.md`
- `specs/3224/tasks.md`

## Risks & Mitigations
- Risk: private/public visibility breakage for test access to report fields.
  - Mitigation: use `pub(super)` on moved structs/fields needed across sibling modules and tests.
- Risk: accidental payload drift in `/gateway/status`.
  - Mitigation: run existing unit/regression/integration tests that assert concrete payload fields.

## Interfaces / Contracts
- `/gateway/status` `multi_channel` payload remains unchanged.
- No route additions/removals.

## ADR
No ADR required (internal module extraction only).
