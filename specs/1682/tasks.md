# Issue 1682 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): run focused `tau-agent-core` helper/runtime test subset for baseline evidence.

T2: extract startup helper implementations to `runtime_startup.rs`.

T3: extract turn-loop helper implementations to `runtime_turn_loop.rs`.

T4: extract tool-bridge helper implementations to `runtime_tool_bridge.rs`.

T5: extract safety/memory helper implementations to `runtime_safety_memory.rs`.

T6: refactor `lib.rs` into lifecycle composition surface + stable helper re-exports.

T7: add split harness and run scoped gates (`cargo test -p tau-agent-core`, strict clippy, fmt, roadmap sync).

## Tier Mapping

- Unit: existing unit tests in `tau-agent-core`
- Functional: lifecycle module split harness
- Integration: runtime/tool/safety/memory flows in crate tests
- Regression: full crate tests + strict lint/format checks
