# Gateway Contract Fixtures

This fixture corpus defines deterministic contract coverage for the Tau gateway service and
external API schema.

## Files

- `mixed-outcomes.json`: success + malformed_input + retryable_failure matrix.
- `rollout-pass.json`: success-only fixture for deterministic rollout/demo runs.
- `invalid-duplicate-case-id.json`: regression fixture for duplicate `case_id`.
- `invalid-error-code.json`: regression fixture for unsupported `error_code`.

## Schema Notes

- Fixture schema version: `1`.
- Supported methods for successful replay: `GET`, `POST`, `PUT`, `PATCH`, `DELETE`.
- Outcome coverage: `success`, `malformed_input`, `retryable_failure`.
- Supported deterministic error codes:
  - `gateway_invalid_request`
  - `gateway_unsupported_method`
  - `gateway_backend_unavailable`
