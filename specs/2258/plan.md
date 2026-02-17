# Plan #2258

Status: Reviewed
Spec: specs/2258/spec.md

## Approach

1. Add `Provider::OpenRouter` to `tau-ai` provider model and parsing.
2. Update auth-command provider parsing to map `openrouter` to `Provider::OpenRouter` and include OpenRouter in auth matrix coverage.
3. Extend provider auth capability/key-resolution logic for OpenRouter using OpenAI-compatible auth mode semantics.
4. Introduce OpenRouter-specific client construction path in `tau-provider`:
   - choose OpenRouter base URL when OpenAI default base is configured
   - keep explicit `--api-base` override behavior
5. Add OpenRouter-specific outbound headers in `tau-ai::OpenAiClient` when target base is OpenRouter (`X-Title`; optional `HTTP-Referer` via env).
6. Implement conformance tests first (RED), then minimal code (GREEN), then regression checks.

## Affected Modules

- `crates/tau-ai/src/provider.rs`
- `crates/tau-ai/src/openai.rs`
- `crates/tau-provider/src/auth_commands_runtime.rs`
- `crates/tau-provider/src/auth.rs`
- `crates/tau-provider/src/client.rs`
- `crates/tau-provider/src/credentials.rs` (if provider-match exhaustiveness requires updates)
- provider/auth integration tests in `crates/tau-coding-agent/tests` and `crates/tau-ai/tests`

## Risks and Mitigations

- Risk: Adding a new provider enum variant can create broad compile/test fallout.
  - Mitigation: keep OpenRouter auth behavior aligned with OpenAI mode/credential flow; update exhaustive matches intentionally and rely on scoped crate tests.
- Risk: header behavior could accidentally affect non-OpenRouter OpenAI calls.
  - Mitigation: gate OpenRouter header injection strictly on OpenRouter base URL detection.
- Risk: routing default surprises users who explicitly set custom base URLs.
  - Mitigation: only substitute to OpenRouter default when configured base equals OpenAI default; otherwise honor user-provided base.

## Interfaces / Contracts

- `Provider` gains `OpenRouter`.
- `parse_auth_provider("openrouter")` returns `Provider::OpenRouter`.
- OpenRouter provider client construction uses OpenAI-compatible transport with OpenRouter defaults/headers.
