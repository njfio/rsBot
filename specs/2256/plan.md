# Plan #2256

Status: Reviewed
Spec: specs/2256/spec.md

## Approach

1. Extend `tau-ai` request/usage types with prompt-caching and cached-token
   fields.
2. Update provider serializers/parsers:
   - OpenAI: request cache-key/retention fields + cached-token usage parse
   - Anthropic: system prompt cache-control blocks + cached-token usage parse
   - Google: `cachedContent` request field + cached-token usage parse
3. Update agent-core cost estimation to use cached-input pricing when available.
4. Thread cached-input model pricing from model catalog into runtime agent
   config.
5. Add conformance tests first, then implement minimum code to satisfy them.

## Affected Modules

- `crates/tau-ai/src/types.rs`
- `crates/tau-ai/src/openai.rs`
- `crates/tau-ai/src/anthropic.rs`
- `crates/tau-ai/src/google.rs`
- `crates/tau-agent-core/src/runtime_turn_loop.rs`
- `crates/tau-agent-core/src/lib.rs`
- `crates/tau-onboarding/src/startup_local_runtime.rs`
- `crates/tau-coding-agent/src/startup_local_runtime.rs`
- `crates/tau-coding-agent/src/training_runtime.rs`

## Risks and Mitigations

- Risk: request-shape regressions across providers.
  - Mitigation: provider-local serializer tests for cache fields and existing
    tool/json-mode behavior.
- Risk: cost regression if cached-token math is wrong.
  - Mitigation: focused unit tests on mixed cached/uncached usage.
- Risk: API drift from provider field names.
  - Mitigation: use provider-native field names in strict parser/serializer
    tests.

## Interfaces / Contracts

- `ChatRequest` adds unified prompt-cache config.
- `ChatUsage` adds `cached_input_tokens`.
- `estimate_usage_cost_usd` includes cached-input-rate parameter.
