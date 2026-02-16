# Plan #2203

Status: Implemented
Spec: specs/2203/spec.md

## Approach

1. Capture RED evidence for stale suppression in `ppo.rs`.
2. Remove stale helper suppression (and dead code) from PPO tests.
3. Generate fresh `allow(...)` inventory and publish wave-2 audit guide.
4. Run scoped `tau-algorithm` check/test/clippy verification.

## Affected Modules

- `specs/milestones/m40/index.md`
- `specs/2203/spec.md`
- `specs/2203/plan.md`
- `specs/2203/tasks.md`
- `crates/tau-algorithm/src/ppo.rs`
- `docs/guides/allow-pragmas-audit-wave2.md`

## Risks and Mitigations

- Risk: removing suppression exposes real warning.
  - Mitigation: run `cargo clippy -D warnings` scoped to `tau-algorithm`.
- Risk: documentation drifts from actual inventory.
  - Mitigation: derive inventory directly from `rg -n "allow\\(" crates -g '*.rs'`.

## Interfaces and Contracts

- Inventory:
  `rg -n "allow\\(" crates -g '*.rs'`
- RED stale suppression evidence:
  `rg -n "#\\[allow\\(dead_code\\)\\]" crates/tau-algorithm/src/ppo.rs`
- Verify:
  `cargo check -p tau-algorithm --target-dir target-fast`
  `cargo test -p tau-algorithm ppo --target-dir target-fast`
  `cargo clippy -p tau-algorithm --target-dir target-fast -- -D warnings`

## ADR References

- Not required.
