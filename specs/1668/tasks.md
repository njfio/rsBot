# Issue 1668 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing deterministic PPO conformance tests for loss terms,
update-step aggregation, and invalid numeric guards.

T2: implement PPO math core in `tau-algorithm` (`compute_ppo_loss`) with
clipping, value term, and entropy term.

T3: implement batch update aggregation (`compute_ppo_update`) with gradient
accumulation summaries.

T4: run fmt/clippy/tests for touched crates and map ACs to passing tests.

## Tier Mapping

- Unit: deterministic loss and guard validation
- Regression: gradient accumulation/update-step determinism
