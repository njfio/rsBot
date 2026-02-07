# Provider Authentication Capability Matrix

Updated: 2026-02-07

## Purpose
This document defines which authentication paths are valid for this project, including whether consumer subscription logins can be used instead of API keys.

## Status Legend
- `Supported now`: official docs support this path and it can be productized.
- `Supported with prerequisites`: official docs support this path, but it requires tooling/account setup checks.
- `Not supported`: no official API auth path for this mode in this project context.

## Capability Matrix
| Provider/Channel | Auth Method | Subscription Login Usable for API Calls | Status | Notes |
|---|---|---|---|---|
| OpenAI API Platform | API key / service account key | No | Supported now | OpenAI API docs define API-key style bearer auth for API requests and mention service account keys in API auth reference. |
| OpenAI consumer subscription products | Web/app account session | No (official API path not documented) | Not supported | No official API docs path for using consumer subscription web sessions as API credentials. This project must not use cookie/session scraping. |
| Anthropic direct API | `x-api-key` header | No | Supported now | Anthropic API examples require key-based auth and version headers on direct API calls. |
| Anthropic on AWS Bedrock | AWS IAM credentials | N/A (cloud IAM path) | Supported with prerequisites | Auth is handled by AWS IAM in Bedrock channel, not Anthropic consumer subscription login. |
| Anthropic on Google Vertex AI | Google IAM/ADC | N/A (cloud IAM path) | Supported with prerequisites | Auth is handled by Google Cloud identity/ADC for Vertex channel. |
| Gemini API (Google AI Studio) | API key | No | Supported now | Gemini docs provide API key setup and API-key based usage guidance. |
| Gemini API (OAuth mode) | User OAuth flow | Partially (OAuth identity, not Gemini Advanced subscription token) | Supported with prerequisites | Official Gemini OAuth docs provide OAuth flow and access token usage patterns. Requires proper client config and token lifecycle handling. |
| Gemini on Vertex AI | ADC / service account | N/A (cloud IAM path) | Supported with prerequisites | Google ADC docs define credential provisioning and login flow for application-default credentials. |

## Source References
- OpenAI API authentication reference: <https://platform.openai.com/docs/api-reference/authentication>
- OpenAI quickstart: <https://platform.openai.com/docs/quickstart>
- Anthropic API getting started: <https://docs.anthropic.com/en/api/getting-started>
- Gemini API key docs: <https://ai.google.dev/gemini-api/docs/api-key>
- Gemini OAuth docs: <https://ai.google.dev/gemini-api/docs/oauth>
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
- Implement provider-neutral auth abstraction before adding login UX.
- Add secure token storage, refresh, revocation, and redaction controls before enabling login flows by default.
- Enforce full unit/functional/integration/regression auth conformance in CI.

## Compliance Constraints
- No reverse-engineered browser session reuse.
- No cookie extraction/scraping workflows.
- Only official provider-documented authentication surfaces are eligible for implementation.
