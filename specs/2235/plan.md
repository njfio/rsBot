# Plan #2235

Status: Implemented
Spec: specs/2235/spec.md

## Approach

1. Add OpenAI endpoint helpers:
   - `chat_completions_url()`
   - `responses_url()`
2. Add Codex routing heuristic:
   - route models containing `codex` to Responses API directly.
3. Add fallback behavior:
   - when chat-completions returns a known model-not-supported message for
     Codex-only models, retry via Responses API once.
4. Add Responses API payload support:
   - build request body with `model`, `input`, and optional controls.
   - parse top-level `status`, `usage`, and `output` message text blocks.
5. Keep non-Codex flow unchanged:
   - existing chat-completions path and stream parsing remain default for
     non-Codex models.
6. Validate with tests and live request:
   - RED tests for routing/parsing/fallback.
   - GREEN implementation.
   - run `cargo test -p tau-ai`.
7. Follow-up hardening from live validation:
   - normalize stringified JSON tool arguments before schema validation.
   - add Anthropic model-specific default `max_tokens` ceilings.
   - forward runtime model catalog `max_output_tokens` through local runtime
     agent settings.
   - add repeatable multi-provider capability matrix harness script + harness
     test.

## Affected Modules

- `crates/tau-ai/src/openai.rs`
- `crates/tau-ai/tests/openai_http_e2e.rs` (if integration additions needed)
- `crates/tau-ai/src/anthropic.rs`
- `crates/tau-agent-core/src/runtime_tool_bridge.rs`
- `crates/tau-onboarding/src/startup_local_runtime.rs`
- `crates/tau-coding-agent/src/startup_local_runtime.rs`
- `crates/tau-coding-agent/src/training_runtime.rs`
- `scripts/dev/live-capability-matrix.sh`
- `scripts/dev/test-live-capability-matrix.sh`

## Risks and Mitigations

- Risk: False-positive routing of non-Codex models.
  - Mitigation: strict model-id check and regression tests.
- Risk: Responses payload variation (string vs structured content).
  - Mitigation: tolerant parser that extracts `output_text` and text blocks.
- Risk: behavior regressions in chat-completions.
  - Mitigation: preserve existing path and add regression conformance case.
- Risk: incorrect provider ceilings can over-constrain or overshoot request
  limits.
  - Mitigation: model-specific mapping with conservative fallback + tests.
- Risk: live harness output can pollute git working tree.
  - Mitigation: route artifacts under `.tau/` and keep outputs untracked.

## Interfaces/Contracts

- No public trait changes (`LlmClient` unchanged).
- `OpenAiClient` internals gain Responses API path and parser utilities only.
