# Plan: Issue #2673 - PRD gateway config GET/PATCH endpoints

## Approach
1. Add route constant + router wiring for `GET/PATCH /gateway/config`.
2. Add typed patch request payload in `types.rs`.
3. Implement `GET` handler to return:
   - active gateway config snapshot from server state,
   - pending override file payload (if present),
   - hot-reload capability metadata.
4. Implement `PATCH` handler to:
   - validate and normalize supported fields,
   - reject empty/invalid payloads,
   - persist pending overrides to gateway state,
   - write runtime heartbeat hot-reload policy file for immediate heartbeat interval updates,
   - return deterministic `applied` vs `restart_required_fields` response.
5. Update `/gateway/status` payload with `config_endpoint`.
6. Add RED-first integration/regression tests for C-01..C-06.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/types.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`
- `specs/milestones/m110/index.md`

## Risks / Mitigations
- Risk: mismatched apply semantics can mislead operators.
  - Mitigation: explicit response schema (`applied` vs `restart_required_fields`) and tests.
- Risk: invalid patches partially persisting state.
  - Mitigation: validate all supported fields before writing overrides/policy files.
- Risk: heartbeat policy updates to wrong path.
  - Mitigation: derive policy path from `runtime_heartbeat.state_path` and assert file writes in tests.

## Interfaces / Contracts
- Endpoint: `GET /gateway/config`
- Endpoint: `PATCH /gateway/config`
- Patch fields (bounded slice):
  - `model?: string`
  - `system_prompt?: string`
  - `max_turns?: usize`
  - `max_input_chars?: usize`
  - `runtime_heartbeat_interval_ms?: u64`
- Persistence:
  - Pending overrides file under gateway openresponses state root.
  - Heartbeat hot-reload policy file at `<runtime_heartbeat.state_path>.policy.toml`.

## ADR
- Not required for this bounded endpoint extension (no new dependency/protocol break).
