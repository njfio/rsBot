# Issue 1738 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing unit/integration/regression tests for checkpoint
promotion gate policy and audit payload logging.

T2: implement trainer-side safety threshold promotion gate evaluator.

T3: implement runtime audit payload helper for threshold decisions.

T4: run fmt/clippy/tests for touched crates and verify AC mapping.

## Tier Mapping

- Unit: threshold enforcement and invalid-policy guards
- Integration: promotion gate evaluation using significance outputs
- Regression: denied-decision audit payload stability
