# Deployment Contract Fixtures

This fixture corpus defines deterministic contract coverage for cloud deployment blueprints and
the WASM runtime deliverable track.

## Files

- `mixed-outcomes.json`: success + malformed_input + retryable_failure matrix.
- `rollout-pass.json`: success-only fixture for deterministic rollout/demo runs.
- `invalid-duplicate-case-id.json`: regression fixture for duplicate `case_id`.
- `invalid-error-code.json`: regression fixture for unsupported `error_code`.

## Schema Notes

- Fixture schema version: `1`.
- Supported deploy targets: `container`, `kubernetes`, `wasm`.
- Supported runtime profiles: `native`, `wasm_wasi`.
- Supported environments: `staging`, `production`.
- Outcome coverage: `success`, `malformed_input`, `retryable_failure`.
- Supported deterministic error codes:
  - `deployment_invalid_blueprint`
  - `deployment_unsupported_runtime`
  - `deployment_missing_artifact`
  - `deployment_backend_unavailable`
