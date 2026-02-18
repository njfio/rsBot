# Plan: Issue #2429 - G4 phase-1 branch tool implementation and validation

## Approach
1. Add RED conformance tests in `tau-tools` for registry coverage and branch tool behavior.
2. Implement `BranchTool` in `crates/tau-tools/src/tools.rs` using existing session append
   primitives and strict input validation.
3. Register the tool in built-in registry/name lists.
4. Run scoped validation gates (`fmt`, `clippy`, targeted tests) and verify conformance mapping.

## Affected Modules
- `crates/tau-tools/src/tools.rs`
- `crates/tau-tools/src/tools/registry_core.rs`
- `crates/tau-tools/src/tools/tests.rs`
- `specs/2429/spec.md`
- `specs/2429/plan.md`
- `specs/2429/tasks.md`
- `specs/milestones/m72/index.md`

## Risks / Mitigations
- Risk: Branch tool behavior duplicates `sessions_send` semantics and introduces ambiguous outcomes.
  - Mitigation: return explicit `tool=branch` and `reason_code=session_branch_created` payload
    with parent/head metadata.
- Risk: Parent-id error handling is non-deterministic.
  - Mitigation: normalize unknown-parent failures to a dedicated reason code.
- Risk: Built-in registry drift (name list vs registration).
  - Mitigation: add explicit C-01 registry test and C-02 runtime execution test.

## Interfaces / Contracts
- New built-in tool contract:
  - name: `branch`
  - args: `{ "path": string, "prompt": string, "parent_id"?: integer }`
  - success payload includes:
    - `tool: "branch"`
    - `reason_code: "session_branch_created"`
    - `selected_parent_id`, `previous_head_id`, `branch_head_id`
- Error payloads include deterministic reason codes for invalid parent and prompt validation.

## ADR
- Not required: no dependency additions or architecture-level protocol changes.
