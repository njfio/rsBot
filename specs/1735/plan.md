# Issue 1735 Plan

Status: Reviewed

## Approach

1. Extend `tau-algorithm` adapter tests with a realistic multi-turn tool trace
   fixture containing user turn, tool call/result, and assistant synthesis.
2. Add explicit assertions for observation/action/reward mapping per step.
3. Add regression assertions for fallback behavior when required span fields are
   missing.

## Affected Areas

- `crates/tau-algorithm/src/adapters.rs`
- `specs/1735/{spec,plan,tasks}.md`

## Risks And Mitigations

- Risk: fixture is too synthetic.
  - Mitigation: include tool-call and tool-result attributes matching runtime
    naming conventions.
- Risk: over-coupling to one exact key set.
  - Mitigation: assert semantic fields and fallback keys, not incidental extras.

## ADR

No architecture/dependency/protocol change. ADR not required.
