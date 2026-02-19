# Spec: Issue #2602 - G4 phase-2 branch tool runtime orchestration + limits

Status: Implemented

## Problem Statement
Issue #2429 delivered a phase-1 `branch` tool that appends branch prompts to session storage, but it did not execute a separate branch reasoning turn or return a branch conclusion to the parent turn. G4 remains incomplete until branch calls can run isolated branch reasoning and report structured conclusions with concurrency safety.

## Acceptance Criteria

### AC-1 Branch tool call executes isolated branch reasoning with memory-only tools
Given a successful `branch` tool call during `Agent::prompt`,
When the runtime handles tool results,
Then it runs one isolated branch reasoning execution that excludes user-facing reply tools and records branch execution metadata in the final tool result payload.

### AC-2 Parent receives structured branch conclusion in tool result
Given branch reasoning completes with an assistant response,
When the parent tool result is appended,
Then the `branch` tool result contains deterministic fields including `reason_code=branch_conclusion_ready` and a non-empty `branch_conclusion` string that the parent turn can consume.

### AC-3 Configurable per-session branch concurrency limit is enforced
Given `AgentConfig.max_concurrent_branches_per_session` is configured,
When branch tool calls exceed the active per-session limit,
Then excess calls fail closed with deterministic `reason_code=branch_concurrency_limit_exceeded` and do not start branch execution.

### AC-4 Branch follow-up failures are deterministic and non-panicking
Given branch follow-up cannot produce a conclusion (for example invalid branch arguments or branch run failure),
When the tool result is recorded,
Then the runtime returns a structured error payload (`reason_code=branch_execution_failed` or `reason_code=branch_prompt_missing`) without panicking and parent execution continues safely.

## Scope

### In Scope
- `tau-agent-core` branch tool follow-up orchestration.
- Branch execution tool-surface restriction (memory-only behavior).
- Branch conclusion payload wiring into parent tool result.
- Per-session branch concurrency limit configuration in `AgentConfig`.
- Conformance + regression tests for AC-1..AC-4.

### Out of Scope
- New `tau-branch` process/service split.
- Cross-session branch scheduling beyond current agent instance.
- New external dependencies, wire-format migrations, or gateway protocol changes.

## Conformance Cases
- C-01 (AC-1, integration): `integration_spec_2602_c01_branch_tool_result_triggers_isolated_branch_followup`
- C-02 (AC-2, functional): `functional_spec_2602_c02_branch_tool_result_contains_structured_branch_conclusion`
- C-03 (AC-3, regression): `regression_spec_2602_c03_branch_tool_enforces_max_concurrent_branches_per_session`
- C-04 (AC-4, regression): `regression_spec_2602_c04_branch_tool_followup_missing_prompt_fails_closed`
- C-05 (AC-3, regression): `regression_spec_2602_c05_branch_concurrency_limit_honors_configured_value_above_one`
- C-06 (AC-3, regression): `regression_spec_2602_c06_branch_slot_released_after_followup_completion`

## Success Metrics / Observable Signals
- Conformance cases C-01..C-06 pass in `tau-agent-core`.
- `cargo fmt --check`, scoped `clippy`, and scoped tests are green.
- No behavioral regressions in existing skip/react/send_file tool-result directive handling.
