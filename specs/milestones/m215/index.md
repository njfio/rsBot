# M215 - Review #35 Gap Correction and Property-Test Depth

Status: In Progress

## Context
`tasks/review-35.md` still lists several unresolved items that are already implemented in Tau (`/cortex/chat` LLM path, provider rate limiting, OpenTelemetry export). The remaining real quality gap is property-based test depth on critical policy/rate-limit paths.

## Scope
- Correct stale unresolved status claims in `tasks/review-35.md` with concrete evidence anchors.
- Add deterministic conformance script coverage for the corrected Review #35 unresolved tracker.
- Add property-based invariant tests for tool rate-limit behavior in `tau-tools`.

## Linked Issues
- Epic: #3150
- Story: #3151
- Task: #3152

## Success Signals
- `tasks/review-35.md` unresolved table matches current implementation state.
- `scripts/dev/test-review-35.sh` passes.
- `cargo test -p tau-tools spec_3152 -- --test-threads=1` passes.
