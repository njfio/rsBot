# Plan #2463 - G16 hot-reload config phase-1 orchestration

## Approach
1. Define M79 milestone container and issue hierarchy.
2. Write bounded story/task specs for runtime heartbeat hot-reload slice.
3. Execute #2465 with RED -> GREEN -> regression flow.
4. Close hierarchy with conformance evidence and milestone closure.

## Risks
- Risk: scope creep into full config hot-reload across modules.
  - Mitigation: keep phase-1 limited to `tau-runtime` heartbeat policy reload behavior.
- Risk: flaky timing tests.
  - Mitigation: assert deterministic snapshot fields with bounded polling helpers.

## Interfaces/Contracts
- Runtime heartbeat scheduler internal policy reload contract (sidecar JSON policy file).
