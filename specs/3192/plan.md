# Plan: Issue #3192 - correct inaccurate PPO unresolved-gap claim in whats-missing report

## Approach
1. Capture direct runtime evidence for PPO/GAE execution paths.
2. Add RED script assertions that require corrected PPO marker and reject stale wording.
3. Update report language to match evidence and rerun script to GREEN.
4. Run fmt/clippy verification.

## Affected Modules
- `tasks/whats-missing.md`
- `scripts/dev/test-whats-missing.sh`
- `specs/milestones/m225/index.md`
- `specs/3192/spec.md`
- `specs/3192/plan.md`
- `specs/3192/tasks.md`

## Risks & Mitigations
- Risk: replacing one inaccurate statement with another.
  - Mitigation: use direct code-path evidence and avoid speculative claims.
- Risk: brittle conformance wording.
  - Mitigation: assert stable markers and keep prose concise.

## Interfaces / Contracts
- Documentation truthfulness contract in `tasks/whats-missing.md`.
- Conformance marker contract in `scripts/dev/test-whats-missing.sh`.

## ADR
No ADR required (docs/script correction only).
