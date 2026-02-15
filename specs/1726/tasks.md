# Issue 1726 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing tests for dual-corruption rollback failure
diagnostics, primary-preferred rollback behavior, and operator diagnostics
rendering contract.

T2: implement deterministic operator diagnostics rendering helper.

T3: adjust rollback diagnostics output only as needed for actionable dual-failure
messages.

T4: run scoped fmt/clippy/tests and map AC-1..AC-3 to C-01..C-03 evidence.

## Tier Mapping

- Integration: primary/fallback restore source selection behavior
- Regression: corrupted primary+fallback failure diagnostics
- Functional: operator-facing diagnostics rendering
- Conformance: C-01..C-03
