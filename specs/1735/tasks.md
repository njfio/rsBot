# Issue 1735 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing multi-turn tool-trace fidelity tests and missing
field fallback assertions.

T2: adjust adapter mapping behavior if required by failing tests.

T3: run scoped fmt/clippy/tests and finalize AC evidence.

## Tier Mapping

- Functional: tool-trace state/action/reward mapping
- Integration: multi-turn trace fidelity across ordered steps
- Regression: deterministic fallback behavior for missing fields
