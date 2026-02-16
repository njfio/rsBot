# Plan #2037

Status: Implemented
Spec: specs/2037/spec.md

## Approach

Use the existing roadmap sync tool as source of truth:

1. Run sync in write mode.
2. Verify generated block output in both docs.
3. Re-run in check mode to confirm deterministic alignment.

## Affected Modules

- `tasks/todo.md`
- `tasks/tau-vs-ironclaw-gap-list.md`
- `scripts/dev/roadmap-status-sync.sh` (execution only, no logic change)

## Risks and Mitigations

- Risk: Manual edits within generated blocks reintroduce drift.
  - Mitigation: Keep check-mode gate usage and avoid manual edits in generated blocks.

## Interfaces and Contracts

- CLI contract: sync command updates generated blocks and check mode detects drift.

## ADR References

- Not required.
