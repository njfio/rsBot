# Issue 1946 Plan

Status: Reviewed

## Approach

1. Extend `PpoConfig` with KL fields (`kl_penalty_coefficient`, `target_kl`,
   `max_kl`) and validate constraints.
2. Extend `PpoLossBreakdown` with `approx_kl` and include KL penalty in
   `total_loss`.
3. Extend `PpoUpdateSummary` with early-stop fields derived from mean KL.
4. Update `compute_ppo_loss` and `compute_ppo_update` math to populate KL
   metrics and guard decisions.
5. Add tests-first coverage for invalid config, KL penalty inclusion, and
   high-divergence early-stop behavior.

## Affected Areas

- `crates/tau-algorithm/src/ppo.rs`
- `specs/1946/spec.md`
- `specs/1946/plan.md`
- `specs/1946/tasks.md`

## Risks And Mitigations

- Risk: KL formula mismatch with PPO conventions.
  - Mitigation: use deterministic approximation from old/new logprobs and
    assert with fixture-based tests.
- Risk: silent behavior changes to existing loss totals.
  - Mitigation: keep defaults backward compatible (zero KL penalty, no max-KL
    stop) and preserve prior outputs under default config.

## ADR

No dependency/protocol changes; ADR not required.
