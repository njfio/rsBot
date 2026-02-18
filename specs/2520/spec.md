# Spec #2520 - Task: implement and validate G13 ReactTool pathway

Status: Implemented

## Problem Statement
G13 remains open because no built-in `react` tool exists even though transport adapters already support reaction delivery semantics.

## Acceptance Criteria
### AC-1 React tool registry and schema
Given built-in tools are registered, when names are enumerated, then `react` is present; when executed with valid args, then response includes normalized `emoji`, optional `message_id`, and a stable reason code.

### AC-2 Turn suppression behavior
Given a successful `react` tool result, when the prompt run completes, then the run terminates without a follow-up assistant text response.

### AC-3 Outbound diagnostics
Given a successful react directive in event execution, when outbound payload is recorded, then payload includes reaction metadata and an explicit reason code.

### AC-4 Adapter compatibility
Given multi-channel reaction dispatch tests, when executed, then existing adapter translation/dispatch remains green.

## Scope
In scope:
- Add `ReactTool` to `tau-tools` and register as built-in.
- Add extraction + suppression handling in `tau-agent-core`.
- Add reaction metadata to `tau-coding-agent` outbound event payload.
- Verify against existing `tau-multi-channel` reaction coverage.

Out of scope:
- New outbound transport support.

## Conformance Cases
- C-01 (AC-1, functional): `spec_2520_c01_builtin_agent_tool_name_registry_includes_react_tool`
- C-02 (AC-1, functional): `spec_2520_c02_react_tool_returns_structured_reaction_payload`
- C-03 (AC-2, integration): `integration_spec_2520_c03_prompt_react_tool_call_terminates_run_without_follow_up_model_turn`
- C-04 (AC-2, unit): `spec_2520_c04_extract_reaction_request_detects_valid_react_tool_payload`
- C-05 (AC-3, integration): `integration_spec_2520_c05_runner_persists_reaction_payload_and_suppresses_text_reply`
- C-06 (AC-4, functional): `functional_runner_executes_tau_react_command_and_records_reaction_delivery`

## Success Metrics
- All C-01..C-06 pass.
- No regressions in skip-tool behavior tests.
