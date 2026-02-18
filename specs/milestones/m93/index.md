# M93 - Spacebot G16 Profile Policy Hot-Reload Bridge (Phase 3)

Milestone: GitHub milestone `M93 - Spacebot G16 Profile Policy Hot-Reload Bridge (Phase 3)`

## Objective
Deliver the next bounded `G16` slice from `tasks/spacebot-comparison.md` by bridging live profile policy changes into runtime heartbeat hot-reload without process restart.

## Scope
- Watch profile TOML changes for the active profile policy fields used by runtime heartbeat.
- Parse and validate updated profile policy values fail-closed.
- Apply valid updates atomically through the existing runtime heartbeat hot-reload path.
- Emit deterministic diagnostics/logging for applied, invalid, and no-op reload outcomes.
- Add conformance/regression coverage and RED/GREEN evidence.

## Out of Scope
- Full profile-wide live hot-reload across all Tau modules.
- Prompt-template watcher integration (`G17` coupling).
- New dependency additions.

## Issue Hierarchy
- Epic: #2539
- Story: #2540
- Task: #2541
- Subtask: #2542

## Exit Criteria
- ACs for the task issue are verified by conformance tests and RED/GREEN evidence.
- `cargo fmt --check`, `cargo clippy -- -D warnings`, scoped tests, and full `cargo test` pass.
- Live validation run succeeds and checklist docs are updated.

## Validation Snapshot (2026-02-18)
- Scoped `tau-coding-agent` checks for `#2541` pass (`fmt`, `clippy`, `spec_2541`, `spec_2542` mutation guard).
- Mutation in diff for `#2541` is clean (`1/1 caught` with `cargo mutants --in-diff`).
- Live validation for advanced capabilities passes (run id `issue2541-live4-1771458590`).
- Workspace `cargo test` is currently blocked by reproducible `tau-runtime` heartbeat hot-reload failures (`spec_2465/spec_2487`) outside `#2541` touched files.
