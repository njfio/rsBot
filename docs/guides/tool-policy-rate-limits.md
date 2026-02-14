## Tool Policy Rate Limits

Date: 2026-02-14  
Story: #1515  
Task: #1516

### Scope

Per-principal rate limiting is enforced for state-changing/high-risk built-in tools:

- `write`
- `edit`
- `bash`

The limiter key is the resolved principal (`ToolPolicy.rbac_principal` when present, otherwise local principal resolution).

### Safe Defaults by Preset

All presets use a fixed one-minute window (`window_ms = 60_000`).

| Preset      | Max Requests / Window | Exceeded Behavior |
|-------------|------------------------|-------------------|
| permissive  | 240                    | defer             |
| balanced    | 120                    | reject            |
| strict      | 60                     | reject            |
| hardened    | 30                     | reject            |

### Policy Fields

`ToolPolicy` exposes:

- `tool_rate_limit_max_requests`
- `tool_rate_limit_window_ms`
- `tool_rate_limit_exceeded_behavior` (`reject` or `defer`)

These can be overridden programmatically after preset application.

### Throttle Error Contract

When throttled, tool responses return structured policy errors with:

- `policy_rule: "rate_limit"`
- `decision: "reject" | "defer"`
- `reason_code: "rate_limit_rejected" | "rate_limit_deferred"`
- `principal`
- `max_requests`, `window_ms`, `retry_after_ms`
- `principal_throttle_events`, `throttle_events_total`

### Observability

- `tool_policy_to_json(...)` includes a `tool_rate_limit` section with config and counters.
- Tool audit events (`tool_execution_end`) include:
  - `throttled` (bool)
  - `throttle_reason_code`
  - `throttle_retry_after_ms`
  - `principal_throttle_events`
  - `throttle_events_total`
  - `throttle_principal`
