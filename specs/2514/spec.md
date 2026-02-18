# Spec #2514 - Task: implement/validate G12 skip-tool conformance

Status: Accepted

## Problem Statement
G12 requires explicit proof that skip behavior suppresses user-facing replies, records skip reasons, and is available in multi-user tool sets.

## Acceptance Criteria
### AC-1 Skip tool payload contract
Given `skip` tool execution, when result is emitted, then payload contains `skip_response=true`, `reason`, and `reason_code=skip_suppressed`.

### AC-2 Turn suppression
Given a `skip` tool call during agent execution, when prompt completes, then the run terminates without a follow-up assistant reply.

### AC-3 Skip reason diagnostics
Given suppressed skip behavior, when outbound event diagnostics are written, then payload includes non-empty `skip_reason` for tuning/debugging.

### AC-4 Toolset inclusion
Given channel/multi-user tool registration, when built-in tools are enumerated, then `skip` is present.

## Scope
In scope:
- `tau-coding-agent` events outbound payload shape for skip diagnostics.
- Conformance validation across `tau-tools`, `tau-agent-core`, and `tau-multi-channel`.

Out of scope:
- New skip semantics or transport protocol redesign.

## Conformance Cases
- C-01 (AC-1, functional): `spec_c02_skip_tool_returns_structured_suppression_payload`
- C-02 (AC-2, integration): `integration_spec_c03_prompt_skip_tool_call_terminates_run_without_follow_up_model_turn`
- C-03 (AC-3, conformance): `spec_2514_c03_events_log_records_skip_reason_for_suppressed_reply`
- C-04 (AC-4, functional): `spec_c01_builtin_agent_tool_name_registry_includes_skip_tool`

## Success Metrics
- C-01..C-04 pass.
- G12 checklist row fully marked complete.
