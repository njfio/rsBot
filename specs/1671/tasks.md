# Issue 1671 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing tests for safety reward mapping, hard-gate reward
clamping, and invalid policy override validation.

T2: implement safety reward policy model and shaping/gating integration in
`TauAgentExecutor`.

T3: add prompt-optimization config `safety_reward` overrides and runtime
validation/application.

T4: update training operations docs with safety reward config fields and
examples.

T5: run scoped verification and map AC-1..AC-4 to C-01..C-04 evidence.

## Tier Mapping

- Unit: policy validation + reason-code penalty mapping
- Functional: executor reward shaping emits deterministic safety penalties
- Integration: adversarial safety reason-code trajectories clamp/deny positive
  reward improvement
- Regression: hard-gate severe violations remain fail-closed
- Conformance: C-01..C-04
