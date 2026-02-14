# Custom Command Contract Fixtures

This fixture corpus defines deterministic contract coverage for no-code custom command authoring
and lifecycle operations.

## Files

- `mixed-outcomes.json`: success + malformed_input + retryable_failure matrix.
- `rollout-pass.json`: all-success fixture for deterministic demo and rollout checks.
- `live-execution-matrix.json`: success + policy deny + retryable failure matrix for live-proof workflows.
- `invalid-duplicate-case-id.json`: regression fixture for duplicate `case_id`.
- `invalid-error-code.json`: regression fixture for unsupported `error_code`.

## Schema Notes

- Fixture schema version: `1`.
- Supported operations: `create`, `update`, `delete`, `run`, `list`.
- Outcome coverage: `success`, `malformed_input`, `retryable_failure`.
- Supported deterministic error codes:
  - `custom_command_invalid_operation`
  - `custom_command_invalid_name`
  - `custom_command_invalid_template`
  - `custom_command_backend_unavailable`
  - `custom_command_policy_denied`
