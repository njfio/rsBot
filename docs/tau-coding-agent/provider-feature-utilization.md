# Provider Feature Utilization (Issue #1189)

Tau now uses provider-specific request features for tool routing, structured JSON output, and Retry-After handling.

## What changed

- Extended `tau-ai::ChatRequest` with provider hint fields:
  - `tool_choice: Option<ToolChoice>`
  - `json_mode: bool`
- Added `ToolChoice` variants:
  - `Auto`
  - `None`
  - `Required`
  - `Tool { name }`

## Runtime behavior updates

- `tau-agent-core` now emits explicit request hints:
  - normal tool-enabled turns use `tool_choice=Auto`
  - `prompt_json` and `continue_turn_json` run with `json_mode=true`
  - structured-output retries remain in `json_mode=true`

## Provider adapter utilization

- OpenAI (`tau-ai/src/openai.rs`)
  - maps `tool_choice` to OpenAI `tool_choice`
  - enables `response_format: { type: \"json_object\" }` when `json_mode=true`
  - uses Retry-After header floor when retrying HTTP status failures

- Anthropic (`tau-ai/src/anthropic.rs`)
  - maps `tool_choice` to Anthropic `tool_choice` (`auto`/`any`/named `tool`)
  - applies JSON-only system steering when `json_mode=true`
  - uses Retry-After header floor when retrying HTTP status failures

- Google (`tau-ai/src/google.rs`)
  - maps `tool_choice` to Gemini `toolConfig.functionCallingConfig`
  - enables `generationConfig.responseMimeType=\"application/json\"` when `json_mode=true`
  - uses Retry-After header floor when retrying HTTP status failures

## Retry-After support

- Added shared helpers in `tau-ai/src/retry.rs`:
  - `parse_retry_after_ms(...)` (seconds and HTTP date)
  - `provider_retry_delay_ms(...)` (max of base backoff and Retry-After floor)

## Test coverage added/updated

- Unit:
  - provider body serialization for `tool_choice` and `json_mode`
  - Retry-After parsing and delay selection helpers
- Functional:
  - structured-output request path sets `json_mode=true`
  - Anthropic/Google named and none tool-choice mappings
- Integration:
  - OpenAI Retry-After floor respected with mocked 429 response
  - request-shaping assertions in `tau-agent-core`
- Regression:
  - tool-choice omission behavior for unsupported/no-tool scenarios
  - generation config merge with JSON mode

## Validation

- `cargo fmt --all`
- `cargo test -p tau-ai`
- `cargo test -p tau-agent-core`
- `cargo test -p tau-provider`
- `cargo test -p tau-runtime`
- `cargo test -p tau-coding-agent run_prompt_with_cancellation`
- `cargo check --workspace`
- `cargo clippy -p tau-ai -p tau-agent-core -p tau-provider -p tau-runtime -p tau-coding-agent --all-targets -- -D warnings`
