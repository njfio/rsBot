# Spec #2256

Status: Implemented
Milestone: specs/milestones/m46/index.md
Issue: https://github.com/njfio/Tau/issues/2256

## Problem Statement

Tau currently sends provider requests without prompt-cache controls and does not
capture cached-token usage from provider responses. As a result, repeated
context is billed at full input rates despite model catalog entries already
tracking cached-input pricing.

## Scope

In scope:

- Add a unified prompt-cache request surface to `ChatRequest`.
- Wire prompt-cache request fields in OpenAI, Anthropic, and Google clients.
- Parse provider cached-token usage into `ChatUsage`.
- Apply cached-input pricing during usage-cost estimation when available.
- Add conformance tests covering request serialization, usage parsing, and cost
  estimation behavior.

Out of scope:

- Google cached-content lifecycle management (create/delete APIs).
- New dependency adoption.
- Session schema redesign.

## Acceptance Criteria

- AC-1: Given prompt caching is enabled on a request, when provider request
  bodies are built, then OpenAI/Anthropic/Google payloads include their
  provider-specific prompt-cache fields.
- AC-2: Given provider responses include cached-token usage metadata, when
  parsed into `ChatResponse`, then `ChatUsage` captures cached input token
  counts.
- AC-3: Given cached-input token counts and cached-input pricing are available,
  when usage cost is estimated, then cached input tokens use
  `cached_input_cost_per_million` while uncached input/output tokens continue to
  use existing rates.

## Conformance Cases

- C-01 (AC-1, unit): OpenAI request body serializes `prompt_cache_key` when
  prompt caching is enabled.
- C-02 (AC-1, unit): Anthropic request body serializes `cache_control` on system
  prompt blocks when prompt caching is enabled.
- C-03 (AC-1, unit): Google request body serializes `cachedContent` when prompt
  caching references an existing cached content id.
- C-04 (AC-2, functional): OpenAI response parser extracts
  `prompt_tokens_details.cached_tokens` into `ChatUsage.cached_input_tokens`.
- C-05 (AC-2, functional): Anthropic response parser extracts
  `usage.cache_read_input_tokens` into `ChatUsage.cached_input_tokens`.
- C-06 (AC-2, functional): Google response parser extracts
  `usageMetadata.cachedContentTokenCount` into
  `ChatUsage.cached_input_tokens`.
- C-07 (AC-3, unit): `estimate_usage_cost_usd` applies cached-input pricing only
  to cached input tokens and preserves existing pricing for uncached tokens.

## Success Metrics / Observable Signals

- Provider request builders emit cache controls without breaking existing tool/
  JSON-mode payload behavior.
- Cached token usage is visible in parsed usage structs.
- Cost estimation decreases when cached-token usage is present and
  cached-input pricing is configured.
