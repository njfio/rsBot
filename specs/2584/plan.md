# Plan #2584

## Approach
1. Enumerate G5/G6/G7 checklist bullets and map each to existing code paths/tests.
2. Run targeted test commands for typed memory, relations, and lifecycle behavior.
3. Patch roadmap checklist statuses only for criteria backed by passing evidence.
4. Capture process logs and pass evidence to #2585 packaging.

## Affected Modules
- `tasks/spacebot-comparison.md`
- `crates/tau-memory/src/runtime.rs`
- `crates/tau-memory/src/runtime/query.rs`
- `crates/tau-tools/src/tools/memory_tools.rs`
- `crates/tau-tools/src/tools/tests.rs`

## Risks & Mitigations
- Risk: checklist claims exceed actual coverage/behavior.
  - Mitigation: only mark items complete when passing test evidence exists.
- Risk: targeted tests miss regressions.
  - Mitigation: run scoped crate gates in #2585 (fmt/clippy/tests + mutation).

## Interfaces / Contracts
- No API changes expected unless evidence reveals a true parity gap.
