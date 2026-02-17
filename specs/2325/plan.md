# Plan #2325

Status: Reviewed
Spec: specs/2325/spec.md

## Approach

1. Capture RED evidence by running the not-yet-created live verifier command.
2. Implement `scripts/dev/verify-session-postgres-live.sh`:
   - start Docker Postgres with random host port mapping;
   - wait until `pg_isready` reports healthy;
   - build DSN and run the three Postgres integration tests in `tau-session`;
   - trap cleanup to stop/remove container on all exits.
3. Update roadmap wave-2 claim #6 to `Resolved` with command evidence.
4. Capture GREEN evidence by executing the verifier end-to-end.

## Affected Modules

- `scripts/dev/verify-session-postgres-live.sh`
- `tasks/resolution-roadmap.md`
- `specs/milestones/m52/index.md`
- `specs/2325/spec.md`
- `specs/2325/plan.md`
- `specs/2325/tasks.md`

## Risks and Mitigations

- Risk: Docker daemon unavailable or slow startup.
  - Mitigation: explicit preflight checks and bounded readiness wait.
- Risk: leaked containers on failure.
  - Mitigation: `trap`-based cleanup that runs on all exits.

## Interfaces / Contracts

- Script contract: non-zero exit on first failed command.
- Test contract: uses existing `tau-session` integration test names unchanged.
- Documentation contract: roadmap claim #6 status/evidence must match executable
  verification.
