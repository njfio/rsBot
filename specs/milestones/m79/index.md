# M79 - Spacebot G16 Hot-Reload Config (Phase 1)

Milestone: GitHub milestone `M79 - Spacebot G16 Hot-Reload Config (Phase 1)`

## Objective
Deliver the first production slice of `tasks/spacebot-comparison.md` gap `G16` by adding deterministic runtime hot-reload behavior for heartbeat scheduler policy without requiring process restart.

## Scope
- Runtime heartbeat scheduler policy reload from a sidecar policy file.
- Reloadable `interval_ms` policy with deterministic apply semantics.
- Invalid policy handling that preserves last-known-good behavior and records diagnostics.
- Conformance coverage for reload/no-reload/error paths.

## Out of Scope
- Full profile-wide hot-reload for all runtime modules.
- Dependency additions (`notify`, `arc-swap`) in this slice.
- Template prompt hot-reload (`G17`).

## Issue Hierarchy
- Epic: #2463
- Story: #2464
- Task: #2465
- Subtask: #2466

## Exit Criteria
- ACs for #2465 are all verified by conformance tests.
- `cargo fmt --check`, `cargo clippy -p tau-runtime -- -D warnings`, and scoped `tau-runtime` test suite pass.
- Milestone issue chain is closed with specs marked Implemented.
