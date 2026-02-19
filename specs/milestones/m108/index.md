# M108 - Spacebot G20 Secret Store Hardening (Phase 2)

Status: Completed
Related roadmap items: `tasks/spacebot-comparison.md` (G20 remaining)

## Objective
Complete remaining G20 backlog by finalizing encrypted-secret storage decisions and migrating API-key-at-rest behavior from plaintext config/profile surfaces to encrypted credential-store flows.

## Issue Map
- Epic: #2655
- Story: #2656
- Task: #2657

## Deliverables
- Define and implement API-key persistence/retrieval paths that use encrypted credential storage by default.
- Preserve existing provider auth UX while preventing plaintext API key persistence in file-backed profile/config artifacts.
- Add conformance/regression coverage for migration behavior.
- Update G20 roadmap checklist evidence for completed remaining items.

## Exit Criteria
- #2655, #2656, and #2657 are closed.
- `specs/2657/spec.md` status is `Implemented`.
- `tasks/spacebot-comparison.md` G20 section is updated with phase-2 evidence.
- Scoped verification gates are green and captured in PR evidence.
