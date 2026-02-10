# Multi-agent Contract Fixtures

These fixtures validate the schema and replay compatibility contract for Tau multi-agent routing.

## Files

- `mixed-outcomes.json`
  - Covers success, malformed input, and retryable failure outcomes.
  - Serves as deterministic replay baseline for contract conformance tests.
- `invalid-error-code.json`
  - Regression fixture ensuring unsupported `expected.error_code` values are rejected.
- `invalid-duplicate-case-id.json`
  - Regression fixture ensuring duplicate `case_id` values are rejected.

## Schema Notes

- `schema_version` must equal `1` for fixture root and each case entry.
- `phase` values:
  - `planner`
  - `delegated_step`
  - `review`
- `expected.outcome` values:
  - `success`
  - `malformed_input`
  - `retryable_failure`
- Supported `expected.error_code` values for non-success outcomes:
  - `multi_agent_invalid_route_table`
  - `multi_agent_empty_step_text`
  - `multi_agent_role_unavailable`
