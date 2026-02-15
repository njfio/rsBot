# Issue 1696 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing edge-case tests for truncation, terminal masking,
and sparse reward stability.

T2: adjust GAE implementation only if edge tests expose defects.

T3: run fmt/clippy/tests and map ACs to passing edge-case conformance tests.

## Tier Mapping

- Unit: truncation and sparse reward stability
- Regression: terminal bootstrap masking
