# Spec #2519 - Story: expose reaction intent via tool contract for channel runs

Status: Implemented

## Problem Statement
Channel behavior supports reactions operationally, but story-level delivery requires a first-class tool contract so agents can request reactions intentionally and suppress redundant text output.

## Acceptance Criteria
### AC-1
Given a `react` tool call with emoji + optional message id, when executed, then the result payload includes structured reaction intent fields and stable reason code.

### AC-2
Given a successful react tool result in a prompt run, when the turn finalizes, then the run ends without generating a follow-up assistant text reply.

### AC-3
Given a react-only run, when runtime diagnostics are emitted, then reaction metadata is present in outbound payload.

## Scope
In scope:
- Tool contract and suppression semantics.
- Diagnostics for observability.

Out of scope:
- New command families beyond react.

## Conformance Cases
- C-01: `spec_2520_c02_react_tool_returns_structured_reaction_payload`
- C-02: `integration_spec_2520_c03_prompt_react_tool_call_terminates_run_without_follow_up_model_turn`
- C-03: `integration_spec_2520_c05_runner_persists_reaction_payload_and_suppresses_text_reply`
