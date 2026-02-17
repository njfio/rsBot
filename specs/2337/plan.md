# Plan #2337

Status: Reviewed
Spec: specs/2337/spec.md

## Approach

1. Capture RED evidence by running the missing dashboard consolidation verifier.
2. Add ADR documenting dashboard consolidation to `tau-gateway`.
3. Add `scripts/dev/verify-dashboard-consolidation.sh` with mapped tests.
4. Update roadmap claim #8 status/evidence to align with the verifier.
5. Execute verifier and capture GREEN evidence.

## Affected Modules

- `docs/architecture/adr-001-dashboard-consolidation.md`
- `scripts/dev/verify-dashboard-consolidation.sh`
- `tasks/resolution-roadmap.md`
- `specs/milestones/m54/index.md`
- `specs/2337/spec.md`
- `specs/2337/plan.md`
- `specs/2337/tasks.md`

## Risks and Mitigations

- Risk: dashboard verification misses an important behavior path.
  - Mitigation: include endpoint, action/audit, stream, and auth regression tests.
- Risk: documentation drift between roadmap and ADR.
  - Mitigation: reference verifier command consistently in both files.

## Interfaces / Contracts

- Verification contract: script is strict/fail-closed and deterministic in test
  sequence.
- ADR contract: explicit Context/Decision/Consequences structure.
