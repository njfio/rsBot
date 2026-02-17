# Spec #2257

Status: Implemented
Milestone: specs/milestones/m46/index.md
Issue: https://github.com/njfio/Tau/issues/2257

## Problem Statement

`tau-algorithm` already ships PPO and GAE implementations, but production prompt-optimization runtime never invokes them. As a result, PPO/GAE remains math-only with zero runtime callers and no operational evidence that policy-optimization updates are executed.

## Scope

In scope:

- Add a production call path from `tau-coding-agent` training runtime into PPO/GAE.
- Add config surface for RL optimizer settings (enable/disable + GAE/PPO knobs).
- Collect trajectories from completed training rollouts and build PPO samples.
- Execute GAE + PPO update and persist deterministic optimizer summary in runtime status artifact.
- Add conformance tests proving the optimizer path is executed and fail-closed on invalid config.

Out of scope:

- Persisting model weights to external model stores.
- Implementing distributed optimizer workers.
- Replacing prompt-optimization loop with full actor-learner architecture.

## Acceptance Criteria

- AC-1: Given prompt-optimization runtime is started with RL optimizer enabled, when training rollouts finish, then runtime invokes trajectory collection + GAE + PPO update in the production path.
- AC-2: Given PPO/GAE runs, when status artifacts are written, then persisted report includes deterministic optimizer summary (trajectory count, sample count, mean PPO loss/kl, early-stop flag).
- AC-3: Given invalid RL optimizer config, when startup validation runs, then runtime fails closed before worker execution with actionable error text.
- AC-4: Given RL optimizer is disabled or there are no eligible train rollouts, when runtime completes, then existing behavior remains intact and optimizer summary is omitted or explicitly marked skipped.

## Conformance Cases

- C-01 (AC-1, unit): training config parser accepts `rl_optimizer` object and materializes defaults/overrides.
- C-02 (AC-1, integration): prompt-optimization runtime with `rl_optimizer.enabled=true` executes optimizer path and returns summary with non-zero trajectory/sample counts.
- C-03 (AC-2, functional): persisted status artifact includes optimizer summary fields (`trajectories`, `samples`, `mean_total_loss`, `observed_approx_kl`, `early_stop_triggered`).
- C-04 (AC-3, unit): invalid PPO config (e.g., `mini_batch_size=0` or `epochs=0`) fails config validation with fail-closed error.
- C-05 (AC-4, regression): disabled optimizer leaves status artifact schema backward-compatible and does not fail training completion.
- C-06 (AC-4, functional): when train rollouts have no usable trajectories/samples, runtime reports optimizer skipped with explicit reason instead of panic.

## Success Metrics / Observable Signals

- `run_prompt_optimization_mode_if_requested` has a direct production caller path into `compute_gae_*` and `compute_ppo_update`.
- Runtime status/report exposes optimizer execution evidence for operators.
- New conformance tests C-01..C-06 pass in CI and guard the call path.
