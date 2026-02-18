# Spec #2518 - Epic: G13 react-tool closure + validation

Status: Implemented

## Problem Statement
Tau already supports reaction delivery at channel adapter/runtime layers, but lacks a first-class agent tool contract for reaction intent. This prevents deterministic LLM-initiated reaction-only acknowledgements.

## Acceptance Criteria
### AC-1 Tool contract exists
Given built-in tool registration, when tools are enumerated, then `react` is present with a stable JSON schema.

### AC-2 Reaction intent suppresses textual reply
Given a successful `react` tool execution during a turn, when the run completes, then no follow-up assistant text reply is emitted for that turn.

### AC-3 Diagnostics preserve reaction intent
Given a successful `react` directive, when outbound event diagnostics are written, then the payload records reaction metadata for audit/tuning.

### AC-4 Adapter compatibility remains valid
Given existing channel adapter reaction dispatch contracts, when conformance tests run, then platform-specific emoji/message-id translation remains green.

## Scope
In scope:
- `tau-tools` react tool contract + registration.
- `tau-agent-core` turn suppression wiring for react directives.
- `tau-coding-agent` outbound event diagnostics for reaction directives.
- Conformance checks against existing `tau-multi-channel` reaction dispatch tests.

Out of scope:
- New transport implementations.
- Broad command protocol redesign.

## Conformance Cases
- C-01 (AC-1, functional): `spec_2520_c01_builtin_agent_tool_name_registry_includes_react_tool`
- C-02 (AC-1, functional): `spec_2520_c02_react_tool_returns_structured_reaction_payload`
- C-03 (AC-2, integration): `integration_spec_2520_c03_prompt_react_tool_call_terminates_run_without_follow_up_model_turn`
- C-04 (AC-2, unit): `spec_2520_c04_extract_reaction_request_detects_valid_react_tool_payload`
- C-05 (AC-3, integration): `integration_spec_2520_c05_runner_persists_reaction_payload_and_suppresses_text_reply`
- C-06 (AC-4, functional): `functional_runner_executes_tau_react_command_and_records_reaction_delivery`

## Success Metrics
- C-01..C-06 pass.
- `tasks/spacebot-comparison.md` G13 pathway is fully checked.
