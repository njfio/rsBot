# M102 - Spacebot G16 Hot-Reload Completion

Status: In Progress
Related roadmap items: `tasks/spacebot-comparison.md` -> G16 (profile config hot-reload)

## Objective
Complete the remaining unchecked G16 parity work by wiring notify-based profile file watching into runtime configuration hot-reload with atomic `ArcSwap` updates and deterministic diagnostics.

## Issue Map
- Epic: #2595
- Story: #2596
- Task: #2597
- Subtask: #2598

## Deliverables
- Notify-backed watcher for active profile TOML changes.
- Atomic config swap path using `ArcSwap` with validation before apply.
- Deterministic diagnostics/logging for `applied`, `no_change`, `invalid`, and watcher/runtime errors.
- Conformance + regression + live validation evidence package.

## Exit Criteria
- Epic/story/task/subtask for M102 closed.
- `specs/<issue>/spec.md` status set to `Implemented` for all M102 issues.
- G16 checklist bullets in `tasks/spacebot-comparison.md` marked validated/complete.
