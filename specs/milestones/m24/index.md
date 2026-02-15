# M24: True RL Pipeline In Production

Milestone: `True RL Wave 2026-Q3: Policy Learning in Production` (`#24`)

## Scope

Deliver a true reinforcement learning pipeline in Tau with policy updates driven
by trajectory rewards, not prompt-search loops.

Core tracks:

- trajectory schema adapters and experience collection
- PPO/GAE optimization primitives and checkpointing
- safety-constrained reward shaping and promotion gates
- operational controls (pause/resume/rollback/recovery)
- benchmark + significance reporting for policy improvements

## Active Spec-Driven Issues (current lane)

- `#1657` Epic: True RL Pipeline for Tau
- `#1658` Story: RL Architecture and Data Model
- `#1659` Story: RL Runner and Experience Collection Loop
- `#1660` Story: PPO Optimizer for LLM Policy Updates
- `#1661` Story: Safety-Constrained RL and Policy Guardrails
- `#1662` Story: RL Evaluation Harness and Benchmark Suite
- `#1663` Story: RL Operations, Rollout Control, and Failure Recovery
- `#1702` Story: Gate M24 True RL Exit

## Contract

Each implementation issue under this milestone must include:

- `specs/<issue-id>/spec.md`
- `specs/<issue-id>/plan.md`
- `specs/<issue-id>/tasks.md`

No implementation is complete until acceptance criteria map to conformance
tests and PR evidence captures red/green execution.
