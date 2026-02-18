# Spec: Issue #2429 - G4 phase-1 branch tool implementation and validation

Status: Implemented

## Problem Statement
Tau has session branching primitives and slash-command navigation (`/branch`), but there is no
first-class built-in `branch` tool in the model tool registry. That prevents model-driven flows
from explicitly creating branch entries through tool-calling and leaves G4 branch-as-tool parity
incomplete.

## Acceptance Criteria

### AC-1 Built-in tool registry includes `branch`
Given Tau registers built-in tools via `register_builtin_tools`,
When the built-in tool catalog is inspected,
Then `branch` is present in `builtin_agent_tool_names()` and can be resolved as a registered tool.

### AC-2 `branch` tool appends a branch prompt with structured metadata
Given `branch` is invoked with valid `path` and `prompt` inputs,
When the tool executes,
Then it appends one user message into the target session branch lineage and returns a success
payload containing `reason_code`, selected parent id, prior head id, and new branch head id.

### AC-3 `branch` supports explicit parent targeting
Given `branch` is invoked with a valid explicit `parent_id`,
When the tool executes,
Then the appended entry is linked to the requested parent and the payload reports that parent id.

### AC-4 `branch` rejects unknown parent ids deterministically
Given `branch` is invoked with a `parent_id` that does not exist in the target session,
When the tool executes,
Then it returns an error payload with deterministic `reason_code` and no session append.

### AC-5 `branch` rejects empty prompt input
Given `branch` receives an empty or whitespace-only `prompt`,
When the tool executes,
Then it returns a validation error and does not append entries.

## Scope

### In Scope
- Add `branch` built-in tool implementation in `tau-tools`.
- Register `branch` in built-in tool name registry and built-in registration flow.
- Reuse existing session storage/append primitives from `tau-session`.
- Add conformance/regression tests for AC-1..AC-5.

### Out of Scope
- Running a secondary model-completion loop for autonomous branch reasoning.
- Cross-session orchestration features (worker delegation, process manager, cortex).
- Any new dependency, schema, protocol, or wire-format changes.

## Conformance Cases
- C-01 (AC-1, unit): `spec_c01_builtin_agent_tool_name_registry_includes_branch_tool`
- C-02 (AC-2, functional): `spec_c02_branch_tool_appends_prompt_and_returns_branch_metadata`
- C-03 (AC-3, integration): `integration_spec_c03_branch_tool_accepts_explicit_parent_id`
- C-04 (AC-4, regression): `regression_spec_c04_branch_tool_rejects_unknown_parent_id`
- C-05 (AC-5, regression): `regression_spec_c05_branch_tool_rejects_empty_prompt`

## Success Metrics / Observable Signals
- Conformance tests C-01..C-05 pass.
- `cargo fmt --check`, scoped `clippy`, and scoped `cargo test -p tau-tools` pass.
- No regression in existing session tools (`sessions_send`, `undo`, `redo`) within touched crate
  test suite.
