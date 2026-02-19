# M104 - Tau Gaps Revalidation and Remediation Wave

Status: In Progress
Related roadmap items: `tasks/tau-gaps-issues-improvements.md` (validation/remediation stream), `tasks/spacebot-comparison.md` (gap inventory reference)

## Objective
Revalidate the Tau gap inventory against current `main`, close stale claims with test-backed evidence, and deliver the highest-priority open remediation slices with spec-driven/TDD artifacts and repeatable validation harnesses.

## Issue Map
- Epic: #2605
- Story: #2606
- Task: #2607
- Task: #2608
- Task: #2609
- Task: #2610
- Task: #2612

## Deliverables
- Evidence-backed status matrix for each tracked gap item (done/partial/open with references).
- Hardened safety and hygiene artifacts required by the roadmap stream.
- Repository-level integration bootstrap under `tests/integration` covering agent loop -> tool execution -> memory write -> memory query retrieval.
- Expanded AC-mapped tests in under-covered crates (`tau-diagnostics`, `tau-training-proxy`, `tau-provider` context/provider helpers).
- Branch hygiene automation for stale merged remote branch inventory + safe prune controls.
- Runtime log sanitization audit + regression checks for secret/token leakage prevention in observability logs.
- Follow-up issues for remaining non-trivial architecture gaps that cannot be closed in one delivery slice.

## Exit Criteria
- #2605, #2606, #2607, #2608, #2609, #2610, and #2612 closed.
- `specs/2608/spec.md` status set to `Implemented`.
- `specs/2609/spec.md` status set to `Implemented`.
- `specs/2610/spec.md` status set to `Implemented`.
- `specs/2612/spec.md` status set to `Implemented`.
- M104-linked roadmap artifacts reflect validated status with concrete evidence links.
