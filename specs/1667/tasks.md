# Issue 1667 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing tests for healthy long-running heartbeat refresh
to ensure no false timeout.

T2: update in-memory and sqlite store heartbeat handling to refresh active
attempt heartbeat timestamps.

T3: verify worker/attempt query accuracy after stale recovery and healthy
execution paths.

T4: run fmt/clippy/tests for touched crates and map ACs to passing tests.

## Tier Mapping

- Functional: worker/attempt state visibility
- Integration: stale timeout + requeue path
- Regression: healthy long-running attempt should not false-timeout
