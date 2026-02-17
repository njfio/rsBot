# Tasks #2256

Status: Completed
Spec: specs/2256/spec.md
Plan: specs/2256/plan.md

- T1 (tests first): add failing conformance tests C-01..C-07 in `tau-ai` and
  `tau-agent-core`.
- T2: extend request/usage types for prompt-cache configuration and cached token
  usage.
- T3: implement provider request/response prompt-cache integration (OpenAI,
  Anthropic, Google).
- T4: apply cached-input pricing path in usage-cost estimation.
- T5: wire cached-input model pricing through runtime startup builders.
- T6: run scoped fmt/clippy/tests and map ACs to passing conformance tests.
