# Spec #2310

Status: Implemented
Milestone: specs/milestones/m49/index.md
Issue: https://github.com/njfio/Tau/issues/2310

## Problem Statement

Gateway OpenResponses currently enforces character-length input limits but does not configure `AgentConfig` token preflight ceilings in the request path. This leaves a path where prompts can still reach provider calls and fail later with provider-side token window errors instead of deterministic local fail-fast validation.

## Scope

In scope:

- Configure gateway OpenResponses `AgentConfig` with preflight token limits derived from `max_input_chars`.
- Ensure oversized prompts fail in the local preflight stage (`TokenBudgetExceeded`) before provider dispatch.
- Preserve existing OpenResponses request/response schema and successful request behavior.
- Add conformance tests covering preflight blocking and successful within-budget requests.

Out of scope:

- Prompt truncation or auto-chunking.
- Dynamic model-catalog context window lookup in gateway runtime.
- API schema changes for OpenResponses responses.

## Acceptance Criteria

- AC-1: Given an OpenResponses request whose input passes char-limit checks but exceeds derived token preflight budget, when execution runs, then request fails with gateway runtime error containing token-budget context.
- AC-2: Given an oversized preflight request, when execution runs, then provider client invocation is skipped (fail-fast before dispatch).
- AC-3: Given a request within derived preflight budget, when execution runs, then request succeeds with unchanged response schema.

## Conformance Cases

- C-01 (AC-1, integration): Request at char-limit boundary fails with token budget exceeded error.
- C-02 (AC-2, integration): Panic-on-call provider client remains uninvoked during oversized preflight failure.
- C-03 (AC-3, regression): Within-budget request still returns `200` with standard `response` payload fields.

## Success Metrics / Observable Signals

- Gateway rejects preflight-oversized prompts without waiting for upstream provider failures.
- Existing OpenResponses compatibility tests remain green.
- No additional required fields appear in response payloads.
