# Plan: Issue #2619 - External coding-agent bridge protocol staging (G21)

## Approach
1. Add RED tests for session pooling/reuse, ordered event streaming, and inactivity reaping.
2. Implement `external_coding_agent_bridge_runtime` module in `tau-runtime` with lifecycle + event APIs.
3. Add ADR documenting protocol stage decisions.
4. Run scoped verification gates and map AC/C evidence in PR.

## Affected Modules
- `crates/tau-runtime/src/lib.rs`
- `crates/tau-runtime/src/external_coding_agent_bridge_runtime.rs` (new)
- `docs/architecture/adr-005-external-coding-agent-bridge-protocol.md` (new)
- `specs/2619/spec.md`
- `specs/2619/plan.md`
- `specs/2619/tasks.md`

## Risks / Mitigations
- Risk: pool semantics are too narrow for future gateway integration.
  - Mitigation: expose generic snapshots/events and keep adapters outside this module.
- Risk: timeout behavior becomes non-deterministic.
  - Mitigation: reaper takes explicit timestamp argument for deterministic tests.
- Risk: accidental behavior coupling with unrelated runtime modules.
  - Mitigation: implement isolated additive module with no side effects on existing paths.

## Interfaces / Contracts
- `ExternalCodingAgentBridgeConfig`
- `ExternalCodingAgentBridge`
- `ExternalCodingAgentSessionStatus` + session snapshot
- `ExternalCodingAgentProgressEvent` with monotonic sequence IDs
- Follow-up queue and inactivity reaper APIs

## ADR
- Required: protocol decision ADR in `docs/architecture/adr-005-external-coding-agent-bridge-protocol.md`.
