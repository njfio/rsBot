# Plan: Issue #2647 - External coding-agent subprocess worker support in bridge runtime (G21 phase 3)

## Approach
1. Add RED tests in `tau-runtime` for subprocess spawn/reuse, stdin follow-up forwarding, stdout/stderr event replay, and close/reap termination behavior.
2. Extend `ExternalCodingAgentBridgeConfig` with optional subprocess launch settings while preserving default no-subprocess behavior.
3. Implement subprocess lifecycle management in bridge session records:
   - spawn process on new session when configured,
   - forward follow-ups to stdin,
   - capture stdout/stderr lines into bridge events,
   - detect/record process exit,
   - terminate process during close/reap.
4. Run/adjust gateway external-coding-agent tests to confirm compatibility and map any new bridge error variants.
5. Run scoped verification and update roadmap/spec status artifacts.

## Affected Modules
- `crates/tau-runtime/src/external_coding_agent_bridge_runtime.rs`
- `crates/tau-runtime/src/lib.rs` (if exports/doc updates needed)
- `crates/tau-gateway/src/gateway_openresponses.rs` (error mapping only if required)
- `crates/tau-gateway/src/gateway_openresponses/tests.rs` (compat/behavior coverage updates)
- `specs/milestones/m106/index.md`
- `specs/2647/spec.md`
- `specs/2647/plan.md`
- `specs/2647/tasks.md`

## Risks / Mitigations
- Risk: subprocess handles leak on terminal lifecycle transitions.
  - Mitigation: centralize termination helper called by close/reap and tested via deterministic fixtures.
- Risk: async output capture races with polling.
  - Mitigation: append output events under bridge mutex and assert ordered event IDs in tests.
- Risk: config changes break existing defaults.
  - Mitigation: default subprocess config remains disabled; run existing gateway lifecycle tests unchanged.

## Interfaces / Contracts
- `ExternalCodingAgentBridgeConfig` gains optional subprocess settings.
- Existing bridge method signatures remain stable.
- Existing gateway request/response payload contracts remain stable.

## ADR
- Not required for this incremental phase; ADR-005 already captures staged protocol and explicitly calls out this subprocess follow-up.
