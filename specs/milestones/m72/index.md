# M72 - Spacebot G4 Branch-as-Tool (Phase 1)

Milestone objective: deliver a first-class built-in `branch` tool contract so models can invoke
explicit branch creation through Tau's tool orchestration path with deterministic, auditable
metadata.

## Scope
- Add `branch` as a built-in tool in `tau-tools` and built-in name registry.
- Define structured `branch` tool arguments and result payload contract.
- Implement deterministic session-branch append behavior using existing `tau-session` primitives.
- Add conformance/regression tests for registration, successful execution, parent targeting, and
  invalid parent/prompt handling.

## Out of Scope
- Autonomous branch execution loops that run additional model turns and return synthesized
  conclusions.
- New process-type architecture changes (channel/branch/worker/cortex split).
- Schema migrations, provider routing changes, or onboarding changes.

## Exit Criteria
- Task issue `#2429` has accepted spec, implemented code, and passing verification evidence.
- Conformance cases C-01..C-05 are mapped to passing tests.
- `cargo fmt --check`, scoped `clippy`, and scoped crate tests pass for touched crates.
