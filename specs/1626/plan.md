# Issue 1626 Plan

Status: Reviewed

## Approach

1. Add inventory schema:
   - `tasks/schemas/m21-scaffold-inventory.schema.json`
2. Add scanner script:
   - `scripts/dev/scaffold-inventory.sh`
   - reads candidate list from decision matrix artifact by default
   - computes per-candidate source-size/runtime-reference/test-touchpoint metrics
   - emits deterministic JSON + markdown outputs
3. Add regression/functional test harness:
   - `scripts/dev/test-scaffold-inventory.sh`
4. Commit first snapshot artifacts:
   - `tasks/reports/m21-scaffold-inventory.json`
   - `tasks/reports/m21-scaffold-inventory.md`

## Affected Areas

- `scripts/dev/scaffold-inventory.sh`
- `scripts/dev/test-scaffold-inventory.sh`
- `tasks/schemas/m21-scaffold-inventory.schema.json`
- `tasks/reports/m21-scaffold-inventory.json`
- `tasks/reports/m21-scaffold-inventory.md`
- `specs/1626/spec.md`
- `specs/1626/plan.md`
- `specs/1626/tasks.md`

## Risks And Mitigations

- Risk: nondeterministic file traversal order.
  - Mitigation: deterministic sorting for candidates/files and fixed timestamp override.
- Risk: ownership omissions in source candidates.
  - Mitigation: fail-closed owner validation in scanner.
- Risk: touchpoint counts become brittle.
  - Mitigation: test only deterministic structure + reproducibility, not exact mutable counts.

## ADR

No new dependencies/protocol changes; ADR not required.
