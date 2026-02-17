# Spec #2325

Status: Implemented
Milestone: specs/milestones/m52/index.md
Issue: https://github.com/njfio/Tau/issues/2325

## Problem Statement

The PostgreSQL backend claim is still classified as partial because live
validation depends on manually providing `TAU_TEST_POSTGRES_DSN`. We need a
single deterministic command that provisions Postgres locally, runs the live
tests, and tears down cleanly.

## Scope

In scope:

- Add `scripts/dev/verify-session-postgres-live.sh`.
- Run existing `tau-session` Postgres integration tests (`c02`, `c03`, `c04`)
  against an ephemeral Docker Postgres instance.
- Update `tasks/resolution-roadmap.md` wave-2 claim #6 evidence/status.

Out of scope:

- CI/CD workflow updates.
- Adding new storage backends or changing Postgres schema behavior.

## Acceptance Criteria

- AC-1: Given Docker is available, when running the live verifier, then it
  starts an ephemeral Postgres container, waits for readiness, and always
  attempts teardown via trap on exit.
- AC-2: Given a started ephemeral Postgres, when the verifier runs, then it
  executes `integration_spec_c02..c04` in `tau-session` with
  `TAU_TEST_POSTGRES_DSN` and fail-closed behavior.
- AC-3: Given updated roadmap documentation, when reviewing wave-2 claim #6,
  then status is `Resolved` with evidence pointing to the live verifier and
  mapped tests.
- AC-4: Given current branch state, when running
  `scripts/dev/verify-session-postgres-live.sh`, then the command exits `0`.

## Conformance Cases

- C-01 (AC-1, functional): verifier script exists, is executable, uses strict
  shell flags, readiness polling, and teardown trap.
- C-02 (AC-2, integration): verifier runs
  `integration_spec_c02_postgres_round_trip_preserves_lineage_when_dsn_provided`,
  `integration_spec_c03_postgres_usage_summary_persists_when_dsn_provided`, and
  `integration_spec_c04_postgres_session_paths_are_isolated_when_dsn_provided`
  with injected DSN.
- C-03 (AC-3, conformance): roadmap claim #6 row is updated to `Resolved` with
  command-level evidence referencing the verifier and tests.
- C-04 (AC-4, integration): running the verifier on this branch exits `0`.

## Success Metrics / Observable Signals

- Operators can validate Postgres live persistence with one command.
- Claim #6 no longer requires ad-hoc environment setup.
- Live validation output is reproducible and fail-closed.
