# Plan: Issue #2607 - Revalidate tau-gaps roadmap and implement open P0/P1 hygiene-safety slice

## Approach
1. Revalidate each roadmap item in `tasks/tau-gaps-issues-improvements.md` against current code/docs/issues and classify as `Done`, `Partial`, or `Open`.
2. Add RED-first `tau-safety` tests for obfuscated prompt-injection/leak inputs and redaction edge cases.
3. Implement minimal scanner hardening only where RED tests demonstrate a gap, then run GREEN tests.
4. Add missing repo/operator artifacts: `.env.example`, `CHANGELOG.md`, `rustfmt.toml`.
5. Create and link follow-up issues for remaining non-trivial open items.
6. Update the roadmap document with validated statuses, evidence, and follow-up links.
7. Run verify gates and summarize AC mapping.

## Affected Modules
- `crates/tau-safety/src/lib.rs`
- `.env.example`
- `CHANGELOG.md`
- `rustfmt.toml`
- `tasks/tau-gaps-issues-improvements.md`
- `specs/2607/spec.md`
- `specs/2607/plan.md`
- `specs/2607/tasks.md`
- `specs/milestones/m104/index.md`

## Risks / Mitigations
- Risk: roadmap validation drifts if evidence is incomplete.
  - Mitigation: require concrete references for every item and keep item numbering stable.
- Risk: safety test hardening introduces false positives.
  - Mitigation: add regression tests that assert both positive detections and clean-input non-matches.
- Risk: broad scope causes partial completion.
  - Mitigation: focus implementation on tractable P0/P1 slice and create explicit follow-up issues for larger items.

## Interfaces / Contracts
- `tau-safety` scanner/leak-detector reason-code and redaction behavior remains deterministic.
- `.env.example` documents environment variables without embedding secrets.
- Roadmap contract in `tasks/tau-gaps-issues-improvements.md` remains a single-source status table for item 1..23.

## ADR
- Not required: no new dependencies or protocol/schema changes in this slice.
