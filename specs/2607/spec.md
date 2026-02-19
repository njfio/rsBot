# Spec: Issue #2607 - Revalidate tau-gaps roadmap and implement open P0/P1 hygiene-safety slice

Status: Implemented

## Problem Statement
`tasks/tau-gaps-issues-improvements.md` was authored from an older snapshot and now mixes stale findings with still-open work. This creates planning drift, duplicate work risk, and under-prioritized safety/hygiene gaps. We need a current, evidence-backed roadmap plus immediate remediation for the open P0/P1 slice.

## Acceptance Criteria

### AC-1 Roadmap entries are revalidated with evidence and current status
Given the current repository state at HEAD,
When `tasks/tau-gaps-issues-improvements.md` is reviewed,
Then each prioritized roadmap item (1..23) is marked `Done`, `Partial`, or `Open` with concrete evidence references (file paths and/or issue IDs).

### AC-2 Missing high-priority repo/operator hygiene artifacts are added
Given missing high-priority artifacts identified in the roadmap,
When remediation is applied,
Then `.env.example`, `CHANGELOG.md`, and `rustfmt.toml` exist with practical baseline content aligned to current Tau behavior.

### AC-3 tau-safety test coverage is materially expanded and regression-focused
Given the current `tau-safety` scanner and leak detector behavior,
When security-focused tests are added,
Then `tau-safety` includes additional conformance/regression tests that cover obfuscated prompt-injection/leak variants and redaction edge cases, and all tests pass.

### AC-4 Remaining non-trivial open items are tracked with linked follow-up issues
Given large open roadmap items that cannot be completed in this slice,
When M104 remediation completes,
Then follow-up GitHub issues exist for those items and are linked from the updated roadmap document.

## Scope

### In Scope
- `tasks/tau-gaps-issues-improvements.md` status refresh with evidence for items 1..23.
- Add `.env.example`, `CHANGELOG.md`, and `rustfmt.toml`.
- Expand `tau-safety` conformance/regression test coverage.
- Open and link follow-up issues for remaining open large items.

### Out of Scope
- Full implementation of large architecture items (for example G1 multi-process, G3 cortex, G18 dashboard SPA).
- CI pipeline contract changes.
- Dependency upgrades or new third-party dependency introductions.

## Conformance Cases
- C-01 (AC-1, functional): roadmap file includes validated status for every item 1..23 and references evidence.
- C-02 (AC-2, conformance): `.env.example`, `CHANGELOG.md`, and `rustfmt.toml` exist and contain non-empty baseline content.
- C-03 (AC-3, regression): new `tau-safety` tests for obfuscated prompt-injection/leak inputs pass with deterministic reason-code/redaction assertions.
- C-04 (AC-4, functional): updated roadmap links follow-up issue IDs for each remaining open non-trivial item.

## Success Metrics / Observable Signals
- `cargo test -p tau-safety` passes with expanded test set.
- `cargo fmt --check` and scoped `cargo clippy -p tau-safety -- -D warnings` pass.
- `tasks/tau-gaps-issues-improvements.md` has complete item-by-item validated status/evidence coverage.
