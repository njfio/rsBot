# M248 - live RL intrinsic reward evaluator

Status: In Progress

## Context
`live_rl_runtime` currently emits a coarse binary reward (`-1.0`, `0.25`, `1.0`). Review #35 identified a gap in intrinsic reward evaluation quality for autonomous improvement loops.

## Scope
- Introduce deterministic composite reward scoring for live RL spans.
- Persist reward breakdown dimensions in `live.agent.decision` span attributes.
- Preserve safety hard-gate semantics while improving reward signal granularity.

## Linked Issues
- Epic: #3282
- Story: #3283
- Task: #3284

## Success Signals
- `cargo test -p tau-coding-agent spec_c05_unit_live_reward_breakdown_scores_deterministically`
- `cargo test -p tau-coding-agent spec_c06_functional_live_rollout_span_persists_reward_breakdown`
- `cargo fmt --check`
- `cargo clippy -p tau-coding-agent --no-deps -- -D warnings`
