# RL Benchmark Fixtures

These fixtures provide deterministic workload families for M24 RL benchmark
validation. They are consumed by `tau-trainer` fixture-loading tests and should
remain stable unless explicitly version-bumped.

## Contract

- One fixture file describes one family (`reasoning` or `tool_use`).
- `suite_id` is unique and versioned.
- Each case includes:
  - `case_id` (stable unique identifier)
  - `seed` (>0 deterministic RNG seed)
  - `prompt`
  - `expected_outcome`
  - `scoring_rubric` (dimension->weight map, non-negative, sums to 1.0)

## Files

- `reasoning-suite.json`
- `tool-use-suite.json`
- `invalid-duplicate-case-id.json` (negative test)
- `invalid-rubric-weight.json` (negative test)
- `invalid-missing-field.json` (negative test)
