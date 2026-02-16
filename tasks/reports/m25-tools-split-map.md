# Tools Runtime Split Map (M25)

- Generated at (UTC): `2026-02-16T00:00:00Z`
- Source file: `crates/tau-tools/src/tools.rs`
- Target line budget: `3000`
- Current line count: `3646`
- Current gap to target: `646`
- Estimated lines to extract: `990`
- Estimated post-split line count: `2656`

## Extraction Phases

| Phase | Owner | Est. Reduction | Depends On | Modules | Notes |
| --- | --- | ---: | --- | --- | --- |
| phase-1-fs-edit-memory (Filesystem/edit and memory domain tools) | tools-runtime | 410 | - | tools/fs_tools.rs, tools/edit_tools.rs, tools/memory_tools.rs | Keep read/write/edit/memory tool JSON contracts and policy hooks stable. |
| phase-2-jobs-history-http (Jobs/history/http command tool surfaces) | tools-orchestration | 320 | phase-1-fs-edit-memory | tools/jobs_tools.rs, tools/history_tools.rs, tools/http_tools.rs | Preserve queue/status/cancel behavior and HTTP safety options across call sites. |
| phase-3-bash-policy-gates (Bash execution and policy/approval gate logic) | tools-safety | 260 | phase-2-jobs-history-http | tools/bash_tool.rs, tools/policy_gates.rs | Keep approval, RBAC, protected-path, and rate-limit gates behaviorally identical. |

## Public API Impact

- Keep exported tool type names and trait implementations stable for runtime callers.
- Preserve JSON argument/return contracts for all moved tools.
- Maintain existing policy gate result semantics and error envelopes.

## Import Impact

- Introduce module declarations under crates/tau-tools/src/tools/ with selective re-exports.
- Move domain-specific tool implementations from tools.rs into phased modules.
- Keep shared helper functions centralized to reduce import fan-out during phased extraction.

## Test Migration Plan

| Order | Step | Command | Expected Signal |
| ---: | --- | --- | --- |
| 1 | guardrail-threshold-enforcement: Introduce and enforce tools.rs split guardrail ending at <3000. | scripts/dev/test-tools-domain-split.sh | tools.rs threshold checks fail closed until split target is reached |
| 2 | tools-crate-coverage: Run crate-scoped tau-tools tests after each extraction phase. | cargo test -p tau-tools | tool behavior, safety checks, and serialization tests stay green |
| 3 | runtime-integration: Run cross-crate runtime integration suites that consume tau-tools surfaces. | cargo test -p tau-coding-agent | no regressions in tool wiring and end-to-end command flows |
