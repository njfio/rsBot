# Issue 1682 Plan

Status: Reviewed

## Approach

1. Capture baseline focused test signal for `tau-agent-core` runtime helpers.
2. Extract internal helper implementations into lifecycle modules:
   - `runtime_startup.rs`
   - `runtime_turn_loop.rs`
   - `runtime_tool_bridge.rs`
   - `runtime_safety_memory.rs`
3. Re-export/mount internal helpers from `lib.rs` so existing methods/tests keep stable names.
4. Add split harness script to validate module boundaries and line budget.
5. Run scoped checks (`tau-agent-core` tests, strict clippy for crate, fmt, roadmap sync).

## Affected Areas

- `crates/tau-agent-core/src/lib.rs`
- `crates/tau-agent-core/src/runtime_startup.rs` (new)
- `crates/tau-agent-core/src/runtime_turn_loop.rs` (new)
- `crates/tau-agent-core/src/runtime_tool_bridge.rs` (new)
- `crates/tau-agent-core/src/runtime_safety_memory.rs` (new)
- `scripts/dev/test-agent-core-lib-domain-split.sh` (new)
- `specs/1682/*`

## Risks And Mitigations

- Risk: behavior drift during helper extraction.
  - Mitigation: move implementations with minimal edits and preserve call paths via re-exported names.
- Risk: test visibility break for private helpers.
  - Mitigation: `pub(crate)` re-exports in `lib.rs` for helper names used by in-crate tests.
- Risk: warnings from stale imports after extraction.
  - Mitigation: run strict clippy and prune unused imports.

## ADR

No dependency/protocol/architecture decision beyond internal decomposition. ADR not required.
