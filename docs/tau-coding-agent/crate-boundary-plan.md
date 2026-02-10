# Tau Crate Boundary Plan

This document defines the target crate layout for decomposing `tau-coding-agent` into focused crates,
along with dependency rules and migration phases.

## Goals

- Reduce compile times by shrinking the primary binary crate.
- Isolate integration-heavy runtimes from core logic.
- Make feature ownership clear and enable parallel work.
- Improve test targeting (unit vs. runtime/integration).

## Current Pain Points

- `tau-coding-agent` contains CLI, runtime loops, provider auth, contracts, and multiple integrations.
- Changes in one subsystem trigger rebuilds of unrelated systems.
- Tests are concentrated in a single crate, making targeted validation harder.

## Target Crate Layout (Phase 1-2)

The first two waves match the existing issue set:

1. `tau-core`
   - Pure types, errors, config structs, time helpers, and JSON schema utilities.
   - No network I/O. No filesystem writes except through injectable traits.
   - Shared across all runtimes.

2. `tau-gateway`
   - Gateway HTTP server, auth/session flows, WebSocket control protocol, and fixtures.
   - Depends on `tau-core` and shared runtime traits.

3. `tau-multi-channel`
   - Multi-channel runtime loop, media normalization, live connectors, and fixtures.
   - Depends on `tau-core` and shared runtime traits.

4. `tau-deployment`
   - Deployment runtime loop, WASM packaging, deliverable validation, and fixtures.
   - Depends on `tau-core` and shared runtime traits.

5. `tau-coding-agent` (thin orchestrator)
   - CLI parsing, startup orchestration, and glue for runtime selection.
   - Delegates to feature crates via clear interfaces.

## Target Crate Layout (Future)

Future extraction targets after the current issue set:

- `tau-dashboard`
  - Dashboard runtime and contract fixtures.
- `tau-voice`
  - Voice runtime and contract fixtures.
- `tau-browser-automation`
  - Browser automation runtime and contract fixtures.
- `tau-credentials`
  - Credentials store, provider auth helpers, and encrypted storage.

## Dependency Rules

- `tau-core` has no dependency on `tokio`, `reqwest`, `axum`, or other I/O frameworks.
- Integration crates may depend on `tokio` and `reqwest`, but not on each other directly.
- `tau-coding-agent` depends on all feature crates but keeps minimal logic.
- Contracts and fixtures live with the runtime crate that owns them.

## Migration Phases

### Phase 1: Boundary Planning (Issue #934)
- Map module ownership to target crates.
- Identify shared types for `tau-core`.
- Define integration crate interfaces.

### Phase 2: `tau-core` Extraction (Issue #937)
- Move shared types, errors, and config structs into `tau-core`.
- Add re-exports or adapters in `tau-coding-agent` to limit churn.
- Update unit tests to target `tau-core` where appropriate.

### Phase 3: Runtime Extractions (Issues #935, #936, #932)
- Extract gateway, multi-channel, and deployment runtimes.
- Move fixtures/tests into those crates.
- Keep CLI options in `tau-coding-agent`, but route to new crate entrypoints.

### Phase 4: CLI Slimming (Issue #933)
- Reduce `tau-coding-agent` to orchestration and user entrypoints.
- Confirm all runtime logic lives in feature crates.

## Ownership Map (Initial)

- `gateway_openresponses.rs`, `gateway_ws_protocol.rs`, `gateway_remote_profile.rs` -> `tau-gateway`
- `multi_channel_runtime.rs`, `multi_channel_live_connectors.rs`, `multi_channel_media.rs` -> `tau-multi-channel`
- `deployment_runtime.rs`, `deployment_wasm.rs` -> `tau-deployment`
- `runtime_types.rs`, `time_utils.rs`, `atomic_io.rs`, `runtime_cli_validation.rs` -> `tau-core`

## Validation Strategy

- Each extracted crate has targeted unit tests for pure logic.
- Runtime crates keep integration tests around fixtures and conformance.
- `tau-coding-agent` retains regression tests that span multiple crates.

## Open Questions

- Should we introduce a `tau-runtime` crate for shared runtime loops and retry policies?
- Do we want a `tau-contracts` crate or keep fixtures with their runtime owners?
- Is a `tau-credentials` crate needed sooner due to upcoming subscription auth work?

## Success Criteria

- `tau-coding-agent` depends on focused crates with stable interfaces.
- Build time for `tau-coding-agent` drops due to slimmer dependency graph.
- Feature tests run in their owning crates without pulling the entire workspace.
