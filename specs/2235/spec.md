# Spec #2235

Status: Implemented
Milestone: specs/milestones/m44/index.md
Issue: https://github.com/njfio/Tau/issues/2235

## Problem Statement

`tau-ai` currently sends all OpenAI requests to `/chat/completions`. OpenAI
Codex models such as `gpt-5.2-codex` are served through `/v1/responses`, so
direct OpenAI Codex calls fail even though equivalent traffic works through
OpenRouter.

Post-implementation live validation also identified two follow-up gaps:
provider defaults for `max_tokens` were too conservative for some Anthropic
models, and local runtime execution did not consistently forward catalog
`max_output_tokens` into outgoing requests.

## Acceptance Criteria

- AC-1: Given a Codex model id, when `OpenAiClient::complete` is called, then
  Tau sends the request to OpenAI Responses API instead of chat-completions.
- AC-2: Given a successful OpenAI Responses payload, when parsed by Tau, then
  the returned `ChatResponse` contains assistant text, finish reason/status,
  and mapped usage tokens.
- AC-3: Given non-Codex chat model ids, when `OpenAiClient::complete` is
  called, then existing chat-completions behavior is preserved.
- AC-4: Given chat-completions returns a model-not-supported style error for a
  Codex-capable model, when fallback is enabled, then Tau retries via
  Responses API and succeeds.
- AC-5: Given these flows, when tests are run, then conformance/unit and
  integration checks pass for new routing without regression to existing tests.
- AC-6: Given Anthropic requests without explicit `max_tokens`, when the model
  is known, then Tau uses a provider/model-specific safe default ceiling.
- AC-7: Given local runtime startup with model catalog output-token metadata,
  when request payloads are built, then `max_tokens` is forwarded.
- AC-8: Given live multi-provider task-completion validation, when executed in a
  repeatable harness, then pass/fail summaries and artifact checks are emitted.

## Scope

In scope:

- OpenAI routing and fallback for Codex model support
- Responses API request body creation and response parsing in `tau-ai`
- tests covering routing, parser, and regression behavior
- Anthropic default token-ceiling mapping and fallback behavior
- local runtime max-token propagation from model catalog metadata
- repeatable live capability matrix script and deterministic harness test

Out of scope:

- Streaming token-by-token support for Responses API event format
- Provider-wide routing abstractions beyond OpenAI
- Model capability discovery automation across all providers

## Conformance Cases

- C-01 (AC-1, Unit): Codex request is sent to `/v1/responses`.
- C-02 (AC-2, Unit): Responses payload with output text and usage maps into
  `ChatResponse`.
- C-03 (AC-3, Regression): `gpt-4o-mini` continues using
  `/v1/chat/completions`.
- C-04 (AC-4, Integration): chat-completions failure with Codex error triggers
  one retry to `/v1/responses`.
- C-05 (AC-5, Functional): `cargo test -p tau-ai` remains green.
- C-06 (AC-6, Unit): `build_messages_request_body` uses model-specific
  Anthropic default max tokens and falls back for unknown models.
- C-07 (AC-7, Functional): local runtime startup request captures propagated
  `max_tokens` when provided in settings.
- C-08 (AC-8, Integration): capability-matrix harness produces summary rows with
  completion and artifact pass/fail for each configured scenario.

## Success Metrics

- Direct OpenAI Codex request succeeds in local live smoke run.
- No failing existing `tau-ai` tests after integration.
- Anthropic long-generation workflows do not fail due to low default max tokens.
- Capability matrix harness can be rerun without repository-output churn.
