# Issue 1946 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing PPO tests for KL penalty term, invalid KL config,
and max-KL early-stop behavior.

T2: extend PPO config/loss/update structs with KL fields and defaults.

T3: implement KL metric computation + penalty + early-stop signaling.

T4: run scoped verification and map AC-1..AC-4 to C-01..C-04.

## Tier Mapping

- Unit: KL config validation and field invariants
- Functional: KL penalty inclusion in total loss
- Integration: deterministic fixture-based update summary checks
- Regression: high-divergence max-KL early-stop path
- Conformance: C-01..C-04
