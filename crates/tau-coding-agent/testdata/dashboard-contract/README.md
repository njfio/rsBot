# Dashboard Contract Fixtures

This fixture corpus defines deterministic contract coverage for the web dashboard/operator control
plane schema.

## Files

- `mixed-outcomes.json`: success + malformed_input + retryable_failure matrix.
- `snapshot-layout.json`: deterministic success-only replay fixture.
- `invalid-duplicate-case-id.json`: regression fixture for duplicate `case_id`.
- `invalid-error-code.json`: regression fixture for unsupported `error_code`.

## Schema Notes

- Fixture schema version: `1`.
- Mode coverage: `snapshot`, `filter`, `control`.
- Outcome coverage: `success`, `malformed_input`, `retryable_failure`.
