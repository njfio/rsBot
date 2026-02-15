# Tool Name Registry

Date: 2026-02-14  
Story: #1442  
Task: #1443

## Scope

Tau reserves built-in tool names so dynamic registrations cannot shadow runtime behavior.

Reserved agent tool names:

- `read`
- `write`
- `edit`
- `sessions_list`
- `sessions_history`
- `sessions_search`
- `sessions_stats`
- `sessions_send`
- `http`
- `bash`

Reserved MCP tool names:

- `tau.read`
- `tau.write`
- `tau.edit`
- `tau.http`
- `tau.bash`
- `tau.context.session`
- `tau.context.skills`
- `tau.context.channel-store`

## Runtime behavior

- Extension runtime registration denies any extension tool whose name is in the reserved agent tool registry.
- MCP external tool discovery denies any tool whose raw external name matches a reserved MCP tool name.
- MCP external tool discovery also denies duplicate fully-qualified external registrations (for example `external.mock.echo` reported twice).

## Deterministic diagnostics

Conflict errors/diagnostics include the exact reserved name so operators can remediate manifests/config quickly.

- Extension diagnostics: `name conflicts with reserved built-in tool '<name>'`
- MCP external discovery errors: `returned reserved built-in tool name '<name>'`
