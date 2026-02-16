# Issue 1681 Spec

Status: Accepted

Issue: `#1681`  
Milestone: `#21`  
Parent: `#1636`

## Problem Statement

`crates/tau-skills/src/package_manifest.rs` remained monolithic and did not
separate schema models, validation helpers, and IO/loading paths into explicit
runtime domains under `crates/tau-skills/src/package_manifest/`.

## Scope

In scope:

- add `specs/1681/{spec,plan,tasks}.md`
- extract schema models into `package_manifest/schema.rs`
- extract validation/parsing helpers into `package_manifest/validation.rs`
- extract IO/loading helpers into `package_manifest/io.rs`
- preserve command behavior and error contracts
- verify split boundaries and line budget with a harness

Out of scope:

- changing package command semantics
- changing wire formats or CLI flag contracts
- introducing new package-manifest features

## Acceptance Criteria

AC-1 (line budget):
Given `crates/tau-skills/src/package_manifest.rs`,
when split is complete,
then line count is below `3200`.

AC-2 (domain extraction):
Given schema/validation/IO boundaries,
when reviewed,
then `package_manifest.rs` wires `mod schema; mod validation; mod io;` and the
three extracted files exist under `crates/tau-skills/src/package_manifest/`.

AC-3 (behavior parity):
Given package-manifest tests,
when targeted tests run,
then package command behavior and validation contracts remain green.

AC-4 (verification):
Given issue-scope checks,
when run,
then harness + targeted tests + roadmap/fmt/clippy pass.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given split result, when harness runs, then `package_manifest.rs < 3200` lines. |
| C-02 | AC-2 | Functional | Given source tree, when harness runs, then required module markers/files are present. |
| C-03 | AC-3 | Regression | Given package-manifest tests, when run, then behavior remains unchanged. |
| C-04 | AC-4 | Integration | Given issue commands, when run, then harness + tests + roadmap/fmt/clippy are green. |

## Success Metrics

- package-manifest schema/validation/IO responsibilities are explicitly modular
- root file drops below threshold with preserved behavior
- issue closure includes explicit spec-driven conformance evidence
