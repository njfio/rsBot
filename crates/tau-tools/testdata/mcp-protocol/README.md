# MCP Protocol Fixtures

These fixtures cover Tau's MCP server mode framing and baseline JSON-RPC method handling.

## Framing

Tau's MCP server mode uses `Content-Length` stdio framing:

1. Header line: `Content-Length: <bytes>`
2. Blank separator line
3. JSON-RPC payload bytes

## Fixture schema

Each fixture JSON file follows this schema:

- `schema_version`: Fixture schema version (currently `1`)
- `name`: Fixture name
- `requests`: Ordered JSON-RPC request frames
- `expected_response_ids`: Ordered response ids expected from server output
- `expected_methods`: Ordered method names used to build the request stream

The fixture tests verify request/response ordering and deterministic server behavior.
