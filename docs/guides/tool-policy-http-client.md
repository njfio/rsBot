# Tool Policy HTTP Client

Date: 2026-02-15  
Story: #1446  
Task: #1447

## Scope

`HttpTool` is a built-in agent/MCP tool for bounded outbound HTTP calls with SSRF guardrails.

Supported methods:

- `GET`
- `POST`
- `PUT`
- `DELETE`

Runtime controls:

- HTTPS-only by default (`http://` blocked unless explicitly allowed).
- Per-request timeout bounded by policy cap.
- Response body byte cap bounded by policy cap.
- Redirects are followed manually with per-hop SSRF re-validation.

## CLI Controls

- `--http-timeout-ms`
- `--http-max-response-bytes`
- `--http-max-redirects`
- `--http-allow-http`
- `--http-allow-private-network`

## Tool Policy JSON Fields

- `http_timeout_ms`
- `http_max_response_bytes`
- `http_max_redirects`
- `http_allow_http`
- `http_allow_private_network`

## Deterministic Reason Codes

Policy and runtime error payloads include stable reason codes:

- `delivery_ssrf_blocked_scheme`
- `delivery_ssrf_blocked_private_network`
- `delivery_ssrf_blocked_metadata_endpoint`
- `delivery_ssrf_dns_resolution_failed`
- `http_invalid_method`
- `http_invalid_headers`
- `http_body_not_allowed`
- `http_timeout_exceeds_policy`
- `http_response_cap_exceeds_policy`
- `http_redirect_limit_exceeded`
- `http_redirect_missing_location`
- `http_redirect_invalid_location`
- `http_response_too_large`
- `http_request_timeout`
- `http_transport_error`

## Live Validation

Example live run:

```bash
cargo run -p tau-coding-agent -- \
  --http-allow-http=true \
  --http-allow-private-network=true \
  --http-timeout-ms 5000 \
  --http-max-response-bytes 4096
```
