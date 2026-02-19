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
- Task: #2611
- Task: #2612
- Task: #2613
- Task: #2616

## Deliverables
- Evidence-backed status matrix for each tracked gap item (done/partial/open with references).
- Hardened safety and hygiene artifacts required by the roadmap stream.
- Repository-level integration bootstrap under `tests/integration` covering agent loop -> tool execution -> memory write -> memory query retrieval.
- Expanded AC-mapped tests in under-covered crates (`tau-diagnostics`, `tau-training-proxy`, `tau-provider` context/provider helpers).
- Branch hygiene automation for stale merged remote branch inventory + safe prune controls.
- Outbound provider token-bucket throttling controls with deterministic burst handling.
- Runtime log sanitization audit + regression checks for secret/token leakage prevention in observability logs.
- Gateway OpenResponses auth secret-id migration for encrypted credential-store backed token/password resolution.
- OpenTelemetry-compatible trace/metric export path for prompt runtime telemetry and gateway runtime cycle events.
- Follow-up issues for remaining non-trivial architecture gaps that cannot be closed in one delivery slice.

## Exit Criteria
- #2605, #2606, #2607, #2608, #2609, #2610, #2611, #2612, #2613, and #2616 closed.
- `specs/2608/spec.md` status set to `Implemented`.
- `specs/2609/spec.md` status set to `Implemented`.
- `specs/2610/spec.md` status set to `Implemented`.
- `specs/2611/spec.md` status set to `Implemented`.
- `specs/2612/spec.md` status set to `Implemented`.
- `specs/2613/spec.md` status set to `Implemented`.
- `specs/2616/spec.md` status set to `Implemented`.
- M104-linked roadmap artifacts reflect validated status with concrete evidence links.
