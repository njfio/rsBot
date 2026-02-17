# Plan #2315

Status: Reviewed
Spec: specs/2315/spec.md

## Approach

1. Create a deterministic verification script under `scripts/dev/` that runs targeted tests for each critical claim.
2. Ensure the script is executable and uses strict shell flags (`set -euo pipefail`) so failures fail closed.
3. Update `tasks/resolution-roadmap.md` with a dated critical-gap revalidation table mapping each claim to status and exact test evidence.
4. Run the script as integration verification and capture output for PR evidence.

## Affected Modules

- `scripts/dev/verify-critical-gaps.sh`
- `tasks/resolution-roadmap.md`
- `specs/milestones/m50/index.md`
- `specs/2315/spec.md`
- `specs/2315/plan.md`
- `specs/2315/tasks.md`

## Risks and Mitigations

- Risk: targeted tests are renamed and script drifts.
  - Mitigation: keep command list explicit and update script alongside test renames.
- Risk: roadmap statements become stale again.
  - Mitigation: include date-stamped revalidation section and command-level evidence.

## Interfaces / Contracts

- Script contract: exits non-zero on first failing validation command.
- Documentation contract: each claim row must include status and evidence test(s).
