# Memory Contract Fixtures

This directory contains deterministic fixtures for the Persistent semantic memory and retrieval
contract used by Tau roadmap issue `#759`.

- `mixed-outcomes.json`: valid fixture covering `success`, `malformed_input`, and
  `retryable_failure` outcomes in a single replay run.
- `retrieve-ranking.json`: valid retrieval fixture used to validate deterministic ranking and
  replay compatibility behavior.
- `invalid-duplicate-case-id.json`: invalid fixture with duplicated `case_id` values.
- `invalid-error-code.json`: invalid fixture with an unsupported `error_code`.

Schema notes:

- Top-level `schema_version` must match `MEMORY_CONTRACT_SCHEMA_VERSION`.
- Each case must include a unique `case_id`, mode (`extract` or `retrieve`), scoped identifiers,
  replay inputs, and deterministic expected outputs.
- Supported deterministic error codes:
  - `memory_empty_input`
  - `memory_invalid_scope`
  - `memory_backend_unavailable`
