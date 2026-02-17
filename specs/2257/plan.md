# Plan #2257

Status: Reviewed
Spec: specs/2257/spec.md

## Approach

1. Extend prompt-optimization config with optional `rl_optimizer` section.
2. Add validation for RL optimizer config in `build_trainer_config` stage to fail fast.
3. After trainer completion, collect successful train rollouts and adapt spans to trajectories.
4. Build GAE batches and PPO samples with deterministic fallback values when logprob/value are missing.
5. Execute PPO update using `tau_algorithm::compute_ppo_update` and attach summary to persisted runtime status artifact.
6. Add conformance-first tests for config parsing, fail-closed validation, execution path, and skip behavior.

## Affected Modules

- `crates/tau-coding-agent/src/training_runtime.rs`
- `crates/tau-algorithm/src/collector.rs` (reuse only, no behavior change expected)
- `crates/tau-onboarding/src/startup_local_runtime.rs` (none expected)

## Risks and Mitigations

- Risk: trajectory spans may lack logprob/value estimates.
  - Mitigation: deterministic fallback values (`0.0`) for PPO sample construction; explicit skip reason when sample set is empty.
- Risk: runtime behavior regressions for existing prompt-optimization users.
  - Mitigation: optimizer remains optional and defaults disabled; regression tests verify unchanged baseline behavior.
- Risk: unstable/non-finite optimization math from malformed inputs.
  - Mitigation: strict config validation + finite checks + fail-closed errors from PPO/GAE utilities surfaced with context.

## Interfaces / Contracts

- `TrainingConfigFile` gains `rl_optimizer: Option<RlOptimizerConfigFile>`.
- `TrainingStatusFile` gains optional `rl_optimizer` summary block.
- New helper executes `collect_trajectory_batch` -> GAE -> PPO and returns structured summary or skip reason.
