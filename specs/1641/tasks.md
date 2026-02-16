# Issue 1641 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add fail-closed contract harness and outbound
serialization-failure regression test; capture RED.

T2: patch outbound payload safety enforcement to fail closed in block mode when
serialization fails.

T3: update quickstart docs with explicit fail-closed semantics for inbound,
tool-output reinjection, and outbound payload stages.

T4: run GREEN harness + targeted safety tests + scoped roadmap/fmt/clippy
checks; prepare PR evidence.

## Tier Mapping

- Functional: issue conformance harness against source/tests/docs
- Regression: outbound serialization-failure fail-closed test
- Integration: targeted inbound/tool-output/outbound safety suite
