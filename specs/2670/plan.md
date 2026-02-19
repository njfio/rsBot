# Plan: Issue #2670 - PRD channel lifecycle action gateway endpoint

## Approach
1. Add a new route constant and router binding for `POST /gateway/channels/{channel}/lifecycle`.
2. Introduce a typed request payload in `types.rs` for lifecycle action controls.
3. Implement a new handler that:
   - enforces auth/rate limits,
   - parses and validates channel/action,
   - builds a `tau-multi-channel` lifecycle command config rooted at the gateway `.tau` workspace,
   - executes lifecycle actions and returns a structured JSON response.
4. Add parsing helpers for channel/action mapping and parameter bounds.
5. Update `GET /gateway/status` payload discovery with `channel_lifecycle_endpoint`.
6. Add integration/regression tests (RED first) for C-01..C-06.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/types.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`
- `specs/milestones/m110/index.md`

## Risks / Mitigations
- Risk: wrong state-root path could mutate unexpected files.
  - Mitigation: derive paths from `gateway_state_dir.parent()/multi-channel` and assert state path in tests.
- Risk: invalid action/channel mapping could silently no-op.
  - Mitigation: explicit parser helpers returning typed errors + regression tests for invalid values.
- Risk: online probe parameters could cause long/blocking calls.
  - Mitigation: default offline probing; clamp timeout/attempt values in handler.

## Interfaces / Contracts
- New endpoint: `POST /gateway/channels/{channel}/lifecycle`
- Request body (JSON):
  - `action: "status" | "login" | "logout" | "probe"` (required)
  - `probe_online?: bool`
  - `probe_online_timeout_ms?: u64`
  - `probe_online_max_attempts?: usize`
  - `probe_online_retry_delay_ms?: u64`
- Response body includes `report` mirroring `MultiChannelLifecycleReport` plus gateway context.

## ADR
- Not required for this bounded endpoint addition (no new dependency, no protocol break).
