# Plan: Issue #2638 - Gateway external coding-agent APIs and SSE stream (G21 phase 2)

## Approach
1. Add RED integration tests in `tau-gateway` for session lifecycle, follow-up queue routing, SSE replay, status payload, and timeout cleanup.
2. Extend gateway state with `ExternalCodingAgentBridge` runtime instance + config snapshot helper.
3. Add authenticated router endpoints and request/response parsing in `gateway_openresponses`.
4. Wire endpoint/runtime metadata into gateway status payload.
5. Run scoped verification (`fmt`, `clippy`, `cargo test -p tau-gateway`) and map AC/C evidence in PR.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/types.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`
- `crates/tau-onboarding/src/startup_transport_modes.rs` (config construction wiring)
- `crates/tau-onboarding/src/startup_transport_modes/tests.rs` (config assertion updates)
- `specs/milestones/m105/index.md`
- `specs/2638/spec.md`
- `specs/2638/plan.md`
- `specs/2638/tasks.md`

## Risks / Mitigations
- Risk: SSE behavior drifts from existing gateway conventions.
  - Mitigation: reuse existing `SseFrame` conventions (`done` frame) and deterministic polling path.
- Risk: introducing shared mutable bridge state causes race bugs.
  - Mitigation: reuse `ExternalCodingAgentBridge` internal synchronization and keep gateway handlers thin.
- Risk: timeout cleanup is hard to test deterministically.
  - Mitigation: expose explicit reap endpoint using current timestamp and short timeout config in tests.

## Interfaces / Contracts
- New gateway endpoints under `/gateway/external-coding-agent/...`.
- New gateway request/response payload structs for open/progress/follow-up/drain/reap.
- `GatewayOpenResponsesServerConfig` gains external coding-agent bridge config.

## ADR
- Not required for this incremental integration step; ADR-005 already captures bridge protocol boundary from #2619.
