# Tasks #2235

Status: Implemented
Spec: specs/2235/spec.md
Plan: specs/2235/plan.md

- T1 (RED): Add failing conformance/unit tests for Codex routing, Responses
  parser mapping, and chat fallback behavior.
- T2 (GREEN): Implement Responses endpoint selection/fallback and response
  parsing in `OpenAiClient`.
- T3 (REFACTOR): Keep helper functions isolated and preserve current
  chat-completions stream behavior for non-Codex models.
- T4 (VERIFY): Run `cargo test -p tau-ai` and targeted provider checks.
- T5 (DOC/TRACE): Update issue status logs and include AC-to-test mapping in PR.
- T6 (RED/GREEN): Add and pass regressions for Anthropic model-specific
  `max_tokens` defaults and unknown-model fallback.
- T7 (RED/GREEN): Add and pass local runtime startup regression asserting
  `max_tokens` propagation from runtime settings.
- T8 (INTEGRATION): Add repeatable live capability matrix harness and
  deterministic harness test with untracked output location.
