# Voice Contract Fixtures

This fixture corpus defines deterministic contract coverage for the Tau voice interaction and
wake-word pipeline.

## Files

- `mixed-outcomes.json`: success + malformed_input + retryable_failure matrix.
- `rollout-pass.json`: all-success fixture for deterministic demo and rollout checks.
- `invalid-duplicate-case-id.json`: regression fixture for duplicate `case_id`.
- `invalid-error-code.json`: regression fixture for unsupported `error_code`.

## Schema Notes

- Fixture schema version: `1`.
- Supported modes: `wake_word`, `turn`.
- Outcome coverage: `success`, `malformed_input`, `retryable_failure`.
- Supported wake words: `tau`, `hey tau`.
- Supported locales for non-malformed inputs: `en-US`, `en-GB`.
- Supported deterministic error codes:
  - `voice_empty_transcript`
  - `voice_invalid_wake_word`
  - `voice_invalid_locale`
  - `voice_backend_unavailable`
