# Issue 1675 Plan

Status: Reviewed

## Approach

1. Add `scripts/demo/m24-rl-live-benchmark-proof.sh` to:
   - read baseline/trained reward sample vectors
   - emit baseline/trained benchmark report artifacts
   - call significance generator script
   - build final proof artifact and validate it
2. Include explicit failure analysis section in proof artifact for failed
   significance gates.
3. Add `scripts/demo/test-m24-rl-live-benchmark-proof.sh` with:
   - passing gain scenario
   - failing non-gain scenario (fail-closed + failure analysis)
4. Update training ops guide with one-command live proof flow.

## Affected Areas

- `scripts/demo/m24-rl-live-benchmark-proof.sh` (new)
- `scripts/demo/test-m24-rl-live-benchmark-proof.sh` (new)
- `docs/guides/training-ops.md`
- `docs/README.md`
- `specs/1675/spec.md`
- `specs/1675/plan.md`
- `specs/1675/tasks.md`

## Risks And Mitigations

- Risk: proof artifacts drift from template/validator contracts.
  - Mitigation: run proof validator in script test path.
- Risk: missing significance report linkage.
  - Mitigation: strict artifact-path checks and deterministic field wiring.
- Risk: ambiguous failed-proof diagnostics.
  - Mitigation: include explicit `failure_analysis` fields in failing output.

## ADR

No architecture/protocol/dependency change; ADR not required.
