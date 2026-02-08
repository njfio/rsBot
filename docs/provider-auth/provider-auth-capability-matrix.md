# Provider Authentication Capability Matrix

Updated: 2026-02-08

## Purpose
This document defines which authentication paths are valid for this project, including whether consumer subscription logins can be used instead of API keys.

## Status Legend
- `Supported now`: official docs support this path and it can be productized.
- `Supported with prerequisites`: official docs support this path, but it requires tooling/account setup checks.
- `Not supported`: no official API auth path for this mode in this project context.

## Provider-Neutral Auth Abstraction
The agent now normalizes provider credentials and auth state via a single abstraction so diagnostics and client construction share the same source of truth.

Core fields:
- `method`: auth method in use (`api_key`, `oauth_token`, `session_token`, `adc`).
- `source`: where the credential was sourced (`flag`, `env`, `credential_store`, `none`).
- `expires_unix`: optional expiration timestamp for time-bound credentials.
- `refreshable`: whether a non-revoked refresh token is present.
- `revoked`: whether the credential has been revoked.

Diagnostics (`/auth status` and `/auth matrix`) read from this abstraction and never emit secrets.

## Capability Matrix
| Provider/Channel | Auth Method | Subscription Login Usable for API Calls | Status | Notes |
|---|---|---|---|---|
| OpenAI API Platform | API key / service account key | No | Supported now | OpenAI API docs require API keys via `Authorization: Bearer` headers for API requests. |
| OpenAI consumer subscription products | Web/app account session | No (official API path not documented) | Not supported | No official API docs path for using consumer subscription web sessions as API credentials. This project must not use cookie/session scraping. |
| Anthropic direct API | `x-api-key` header | No | Supported now | Anthropic API docs require `x-api-key` headers on API requests, along with version headers. |
| Anthropic on AWS Bedrock | AWS IAM credentials | N/A (cloud IAM path) | Supported with prerequisites | Auth is handled by AWS IAM in Bedrock channel, not Anthropic consumer subscription login. |
| Anthropic on Google Vertex AI | Google IAM/ADC | N/A (cloud IAM path) | Supported with prerequisites | Auth is handled by Google Cloud identity/ADC for Vertex channel. |
| Gemini API (Google AI Studio) | API key | No | Supported now | Gemini API docs require `x-goog-api-key` for API calls. |
| Gemini API (OAuth mode) | User OAuth flow (via ADC) | Partially (OAuth identity, not Gemini Advanced subscription token) | Supported with prerequisites | Gemini OAuth docs describe using `gcloud auth application-default login` with a client ID file to produce ADC tokens. Requires proper client config and token lifecycle handling. |
| Gemini on Vertex AI | ADC / service account | N/A (cloud IAM path) | Supported with prerequisites | Google ADC docs define credential provisioning and login flow for application-default credentials. |

## Source References
- OpenAI API authentication reference: <https://platform.openai.com/docs/api-reference/authentication?api-mode=responses>
- Anthropic API getting started: <https://docs.anthropic.com/en/api/getting-started>
- Gemini API reference (API key auth): <https://ai.google.dev/api>
- Gemini OAuth quickstart: <https://ai.google.dev/gemini-api/docs/oauth>
- Google ADC docs: <https://cloud.google.com/docs/authentication/provide-credentials-adc>

## Decision Gates for Roadmap Stories
1. OpenAI login mode gate (`#103`, `#115`)
- Keep OpenAI API auth limited to documented API credentials (API key/service account key) until official docs publish a supported user-login API auth mechanism.

2. Anthropic channel gate (`#104`, `#116`)
- Treat direct Anthropic API and cloud-channel IAM as separate capabilities.
- Do not model consumer subscription login as a direct API auth path.

3. Gemini login gate (`#105`, `#117`)
- Allow design/implementation for OAuth and ADC paths with strict preflight validation.
- Ensure capability diagnostics clearly distinguish OAuth identity from consumer subscription entitlements.

4. Cross-provider implementation gates (`#106`, `#107`, `#108`, `#109`, `#118`)
- Provider-neutral auth abstraction is implemented and required before login UX changes.
- Add secure token storage, refresh, revocation, and redaction controls before enabling login flows by default.
- Enforce full unit/functional/integration/regression auth conformance in CI.

## Compliance Constraints
- No reverse-engineered browser session reuse.
- No cookie extraction/scraping workflows.
- Only official provider-documented authentication surfaces are eligible for implementation.
