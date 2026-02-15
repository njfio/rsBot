# MCP Client Operations Guide

This guide covers `--mcp-client` runtime enablement, configuration, OAuth 2.1 PKCE handling, and diagnostics.

## CLI Flags

- `--mcp-client`: enable MCP client discovery/registration for local runtime tools.
- `--mcp-external-server-config <path>`: MCP server/client config path (required for `--mcp-client`).
- `--mcp-client-inspect`: run discovery and diagnostics, then exit.
- `--mcp-client-inspect-json`: render inspect output as JSON.

## Configuration Schema

`schema_version` is required and currently fixed to `1`.

```json
{
  "schema_version": 1,
  "servers": [
    {
      "name": "local_stdio",
      "command": "/absolute/path/to/mcp-server",
      "args": ["--mode", "stdio"]
    },
    {
      "name": "remote_http",
      "transport": "http-sse",
      "endpoint": "https://mcp.example.com/rpc",
      "sse_endpoint": "https://mcp.example.com/sse",
      "headers": {
        "x-tenant": "demo"
      },
      "auth": {
        "type": "oauth_pkce",
        "authorization_url": "https://auth.example.com/authorize",
        "token_url": "https://auth.example.com/token",
        "client_id": "tau-demo-client",
        "redirect_uri": "urn:ietf:wg:oauth:2.0:oob",
        "scopes": ["mcp.tools.read", "mcp.tools.call"],
        "authorization_code": "<auth_code>",
        "code_verifier": "<pkce_code_verifier>"
      }
    }
  ]
}
```

## Tool Naming

Discovered MCP tools are registered as:

- default prefix: `mcp.<server_name>.<tool_name>`
- configurable prefix: `tool_prefix` in server config

Name collisions are skipped with diagnostics (`mcp_client_tool_name_conflict`).

## OAuth Token Handling

For `oauth_pkce` auth:

- Access and refresh tokens are persisted in the standard Tau credential store (`--credential-store`) using the integration key `mcp.oauth.<server_name>`.
- Storage encryption mode follows `--credential-store-encryption` and `--credential-store-key`.
- Refresh attempts occur before expiry using `refresh_skew_seconds` (default `60`).
- If neither a valid token nor refresh token is available, Tau uses configured `authorization_code` and `code_verifier` to exchange a new token.

## Diagnostics and Reason Codes

Inspect and runtime registration emit structured diagnostics with reason codes, including:

- `mcp_client_server_discovered`
- `mcp_client_tool_registered`
- `mcp_client_tool_name_conflict`
- `mcp_client_oauth_authorization_code_missing`
- `mcp_client_oauth_code_verifier_missing`
- `mcp_client_oauth_token_exchange_failed`
- `mcp_client_sse_probe_failed`
- `mcp_client_http_request_failed`
- `mcp_client_jsonrpc_error`
- `mcp_client_invalid_tool_catalog`
- `mcp_client_stdio_transport_failed`

## Live Validation

Inspect mode:

```bash
tau-rs \
  --mcp-client \
  --mcp-external-server-config .tau/mcp/client.json \
  --mcp-client-inspect \
  --mcp-client-inspect-json
```

Runtime mode:

```bash
tau-rs \
  --mcp-client \
  --mcp-external-server-config .tau/mcp/client.json \
  --prompt "List the available MCP tools and call one safely."
```
