# M95 - Spacebot G8 Local Embeddings (Phase 1)

Milestone: GitHub milestone `M95 - Spacebot G8 Local Embeddings (Phase 1)`

## Objective
Close the next Spacebot-comparison priority gap (`G8`) by shipping local embeddings for memory save/search and making local mode the default policy path while preserving explicit remote-provider overrides.

## Scope
- Add local embedding backend support to `tau-memory`.
- Keep deterministic fail-closed fallback behavior to hash embeddings.
- Set default embedding provider policy/profile behavior to `local`.
- Preserve existing remote-provider embedding behavior.
- Ship conformance/regression/mutation/live-validation evidence for the delivered task slices.

## Out of Scope
- Graph ranking changes (`G6`).
- Memory lifecycle policy changes (`G7`).
- UI/dashboard work (`G18+`).

## Issue Hierarchy
- Epic: #2551
- Story: #2552
- Task: #2553
- Task: #2556
- Subtask: #2554

## Exit Criteria
- ACs for #2553 and #2556 are verified by conformance tests.
- `cargo fmt --check`, scoped `clippy`, scoped tests, mutation in diff, workspace `cargo test -j 1 -- --test-threads=1`, and live validation all pass.
- `tasks/spacebot-comparison.md` `G8` checklist is updated for delivered scope.
