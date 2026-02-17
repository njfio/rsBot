# Spec #2258

Status: Implemented
Milestone: specs/milestones/m46/index.md
Issue: https://github.com/njfio/Tau/issues/2258

## Problem Statement

OpenRouter is currently treated as an OpenAI alias. That collapses provider identity and prevents provider-specific defaults and headers. Production users cannot rely on explicit OpenRouter routing semantics, and auth/status surfaces do not represent OpenRouter as a first-class provider.

## Scope

In scope:

- Introduce first-class `Provider::OpenRouter` identity in provider parsing and provider-facing command parsing.
- Route OpenRouter provider requests through OpenRouter default base URL when OpenAI default base URL is configured.
- Apply OpenRouter-specific headers (`X-Title` and optional `HTTP-Referer`) on OpenRouter HTTP requests.
- Keep OpenAI behavior and existing OpenAI-compatible aliases backward-compatible.
- Add conformance and integration tests for parsing, auth handling, routing, and request headers.

Out of scope:

- Making every OpenAI-compatible alias (deepseek/groq/xai/mistral/azure) first-class providers.
- New CLI flag families dedicated to OpenRouter auth mode.
- Changes to Anthropic or Google provider behavior.

## Acceptance Criteria

- AC-1: Given a `model` value prefixed with `openrouter/`, when parsed, then provider identity is `OpenRouter` (not `OpenAi`) while preserving the remaining model path.
- AC-2: Given `/auth` provider tokens, when `openrouter` is parsed, then auth command provider identity is `OpenRouter` and status/matrix commands accept OpenRouter as an explicit provider filter.
- AC-3: Given OpenRouter provider client construction with default OpenAI API base (`https://api.openai.com/v1`), when requests are executed, then runtime routes to OpenRouter API base (`https://openrouter.ai/api/v1`).
- AC-4: Given OpenRouter provider HTTP requests, when outbound requests are made, then `X-Title` is always present and `HTTP-Referer` is included when configured.
- AC-5: Given existing OpenAI and alias model refs, when parsed and executed, then pre-existing OpenAI behavior remains unchanged.

## Conformance Cases

- C-01 (AC-1, unit): `ModelRef::parse("openrouter/openai/gpt-4o-mini")` yields `Provider::OpenRouter` and model `openai/gpt-4o-mini`.
- C-02 (AC-1, regression): `ModelRef::parse("gpt-4o-mini")` still defaults to `Provider::OpenAi`.
- C-03 (AC-2, unit): `parse_auth_provider("openrouter")` yields `Provider::OpenRouter`.
- C-04 (AC-2, functional): `execute_auth_matrix_command` with provider filter `openrouter` emits rows with `provider=openrouter`.
- C-05 (AC-3, integration): OpenRouter provider client uses OpenRouter base when CLI base is default OpenAI base.
- C-06 (AC-4, integration): OpenRouter request includes `X-Title` and optional `HTTP-Referer` headers.
- C-07 (AC-5, regression): OpenAI and alias parse paths continue to resolve as before for non-openrouter inputs.

## Success Metrics / Observable Signals

- `tau-ai::Provider` includes explicit OpenRouter variant in stable parsing paths.
- `tau-provider` auth command parsing and matrix output represent OpenRouter identity.
- `tau-provider` client construction emits OpenRouter-directed requests with provider-specific headers.
- Conformance tests C-01..C-07 pass in CI.
