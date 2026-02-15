# Issue 1737 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing functional/integration/regression tests for safety
penalty calibration and default selection.

T2: implement calibration policy, deterministic ranking, and default selector in
`tau-algorithm`.

T3: add benchmark fixture for calibration observations and integrate it in
tests.

T4: run fmt/clippy/tests for touched crate and verify AC mapping.

## Tier Mapping

- Functional: deterministic ranking + filtering
- Integration: fixture-backed default coefficient selection
- Regression: fail-closed no-compliant-candidate path
