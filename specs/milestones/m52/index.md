# M52 — Postgres Live Verification for Session Backend

Milestone: [GitHub milestone #52](https://github.com/njfio/Tau/milestone/52)

## Objective

Provide a deterministic, local live-run verification path for the `tau-session`
PostgreSQL backend so persistence behavior can be validated without manual DSN
setup.

## Scope

- Add a `scripts/dev` verifier that boots ephemeral PostgreSQL in Docker.
- Execute existing Postgres integration tests (`c02`, `c03`, `c04`) via an
  injected `TAU_TEST_POSTGRES_DSN`.
- Update roadmap status/evidence to reflect reproducible live validation.

## Out of Scope

- CI workflow changes.
- Replacing current session storage implementation details.

## Linked Hierarchy

- Epic: #2322
- Story: #2323
- Task: #2324
- Subtask: #2325
