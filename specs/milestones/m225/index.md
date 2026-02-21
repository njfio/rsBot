# M225 - Whats-Missing PPO Claim Correction

Status: In Progress

## Context
`tasks/whats-missing.md` currently states PPO/GAE is not wired into runtime training. Repository evidence shows PPO/GAE execution is already wired in `tau-coding-agent` training and live RL runtimes.

## Scope
- Correct the inaccurate PPO unresolved-gap claim.
- Update `scripts/dev/test-whats-missing.sh` markers to enforce corrected report language.
- Preserve stale-claim protections for previously resolved items.

## Linked Issues
- Epic: #3190
- Story: #3191
- Task: #3192

## Success Signals
- `scripts/dev/test-whats-missing.sh`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
