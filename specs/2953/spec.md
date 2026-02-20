# Spec: Issue #2953 - Wire `/cortex/chat` to LLM with observer context and fallback

Status: Implemented

## Problem Statement
`/cortex/chat` currently returns deterministic mock output (`render_cortex_output_text`) and does not invoke the configured provider client. This leaves G3 partially closed and blocks real cross-session analytical responses in Cortex admin chat.

## Acceptance Criteria

### AC-1 `/cortex/chat` invokes LLM completion path
Given an authenticated cortex chat request,
When the request is processed,
Then gateway calls `LlmClient::complete` with a bounded `ChatRequest` and emits SSE output from LLM text.

### AC-2 Cortex LLM prompt includes operator context inputs
Given a cortex chat request,
When request messages are built,
Then the LLM context includes observer event summary, bulletin snapshot, and memory graph summary.

### AC-3 Provider failures and empty output degrade safely
Given provider failure or empty LLM output,
When `/cortex/chat` executes,
Then endpoint still returns successful SSE contract using deterministic fallback text and reason-coded metadata.

### AC-4 Existing SSE event contract remains stable
Given successful or fallback execution,
When streaming response,
Then event sequence remains `cortex.response.created`, `cortex.response.output_text.delta`, `cortex.response.output_text.done`, `done`.

## Scope

### In Scope
- `crates/tau-gateway/src/gateway_openresponses/cortex_runtime.rs` LLM wiring.
- Context rendering helpers for observer status and memory graph summary.
- Unit/integration tests for LLM and fallback behavior.

### Out of Scope
- Changing auth, endpoint path, or SSE envelope schema.
- Reworking full Cortex bulletin refresh loop.
- UI changes in webchat page.

## Conformance Cases
- C-01: `/cortex/chat` LLM path emits non-mock output produced by test `LlmClient`.
- C-02: LLM request message payload contains observer/bulletin/memory context markers.
- C-03: provider error path emits fallback output with deterministic reason code.
- C-04: SSE event ordering remains stable.

## Success Metrics / Observable Signals
- `cargo test -p tau-gateway cortex_runtime -- --test-threads=1` passes.
- Gateway integration tests for cortex chat/status remain green.

## Approval Gate
P0 scope is proceeding under explicit user instruction to continue execution end-to-end; PR will explicitly flag this slice for human review.
