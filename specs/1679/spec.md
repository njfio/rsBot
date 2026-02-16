# Issue 1679 Spec

Status: Accepted

Issue: `#1679`  
Milestone: `#21`  
Parent: `#1635`

## Problem Statement

`crates/tau-cli/src/cli_args.rs` remains above the maintainability threshold
(`4342` lines), and this subtask lacks issue-specific spec artifacts and
conformance evidence for domain-oriented decomposition.

## Scope

In scope:

- add `specs/1679/{spec,plan,tasks}.md`
- split `cli_args.rs` by extracting argument-domain sections into
  `crates/tau-cli/src/cli_args/`
- preserve existing CLI field names, clap metadata, and runtime behavior
- verify line-count target and CLI validation coverage

Out of scope:

- changing CLI semantics or default values
- adding/removing flags
- cross-crate refactors unrelated to CLI argument struct decomposition

## Acceptance Criteria

AC-1 (line budget):
Given `crates/tau-cli/src/cli_args.rs`,
when split is complete,
then line count is below `4000`.

AC-2 (domain extraction):
Given CLI tail domains (custom-command deprecated flags, voice flags, github
bridge flags),
when reviewed,
then they are extracted into `crates/tau-cli/src/cli_args/` and included from
`cli_args.rs`.

AC-3 (behavior parity):
Given existing CLI validation tests,
when targeted tests run,
then parse defaults/flag wiring remain unchanged.

AC-4 (verification):
Given issue-scope checks,
when run,
then contract harness + targeted tests + roadmap/fmt/clippy pass.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given split result, when line-count check runs, then `cli_args.rs < 4000` lines. |
| C-02 | AC-2 | Functional | Given source tree, when contract harness runs, then extracted domain module and flatten marker wiring are present. |
| C-03 | AC-3 | Regression | Given targeted CLI validation tests, when run, then defaults/flag parsing behavior is unchanged. |
| C-04 | AC-4 | Integration | Given issue commands, when run, then harness + tests + roadmap/fmt/clippy are green. |

## Success Metrics

- `cli_args.rs` below threshold without behavior regressions
- extracted domains live under `crates/tau-cli/src/cli_args/`
- issue closure contains explicit spec-driven evidence
