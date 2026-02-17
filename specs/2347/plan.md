# Plan #2347

Status: Reviewed
Spec: specs/2347/spec.md

## Approach

1. Create a single verifier script that runs the four mapped `tau-provider`
   conformance/integration/regression tests for catalog discovery and fallback.
2. Capture RED evidence by asserting the roadmap resolved marker is absent.
3. Update the roadmap claim to resolved with command-level evidence.
4. Run verifier script and formatting checks for GREEN evidence.

## Affected Modules (planned)

- `scripts/dev/verify-model-catalog-discovery-claim.sh`
- `tasks/resolution-roadmap.md`
- `specs/milestones/m56/index.md`
- `specs/2347/spec.md`
- `specs/2347/plan.md`
- `specs/2347/tasks.md`

## Risks and Mitigations

- Risk: existing tests may be flaky due network fixture behavior.
  - Mitigation: rely on deterministic local fixtures already in `tau-provider`
    tests and keep verifier script fail-closed.
- Risk: roadmap drift across parallel updates.
  - Mitigation: update only the targeted top-level claim line in this slice.
