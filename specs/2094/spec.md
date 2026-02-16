# Spec #2094

Status: Implemented
Milestone: specs/milestones/m27/index.md
Issue: https://github.com/njfio/Tau/issues/2094

## Problem Statement

Epic M27 governs first-stage CLI argument-surface decomposition for
`tau-cli`, starting with execution-domain extraction from `cli_args.rs` while
preserving compatibility. Story/task/subtask chain (`#2095/#2096/#2097`) has
completed via PRs `#2101/#2100/#2099`; this epic closes by consolidating that
conformance evidence.

## Acceptance Criteria

- AC-1: M27.1 hierarchy (`#2095/#2096/#2097`) is completed and merged.
- AC-2: Execution-domain split is integrated in mainline with compatibility
  checks passing.
- AC-3: Epic-level lifecycle artifacts document AC -> conformance traceability.

## Scope

In:

- consume merged story/task/subtask outputs for M27.1
- map epic ACs to conformance evidence
- close epic with lifecycle artifacts and status handoff

Out:

- additional domain decomposition waves beyond M27.1
- unrelated CLI feature changes

## Conformance Cases

- C-01 (AC-1, governance): issues `#2095`, `#2096`, `#2097` are closed with
  merged PR evidence.
- C-02 (AC-2, functional):
  `cargo check -p tau-cli --lib --target-dir target-fast` passes on latest
  `master`.
- C-03 (AC-2, regression):
  `bash scripts/dev/test-cli-args-domain-split.sh` passes.
- C-04 (AC-2, integration):
  `cargo test -p tau-coding-agent startup_preflight_and_policy --target-dir target-fast`
  passes.

## Success Metrics

- Epic `#2094` closes with linked AC/conformance evidence.
- `specs/2094/{spec,plan,tasks}.md` lifecycle is completed.
- Milestone M27 can advance to next decomposition slice.
