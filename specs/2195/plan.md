# Plan #2195

Status: Implemented
Spec: specs/2195/spec.md

## Approach

1. Add RED wave-12 marker assertions to
   `scripts/dev/test-split-module-rustdoc.sh`.
2. Run guard script and capture expected failure.
3. Add concise rustdoc comments to wave-12 GitHub runtime modules.
4. Run guard, scoped check, and targeted tests.

## Affected Modules

- `specs/milestones/m39/index.md`
- `specs/2195/spec.md`
- `specs/2195/plan.md`
- `specs/2195/tasks.md`
- `scripts/dev/test-split-module-rustdoc.sh`
- `crates/tau-github-issues-runtime/src/github_issues_runtime/demo_index_runtime.rs`
- `crates/tau-github-issues-runtime/src/github_issues_runtime/issue_command_rendering.rs`

## Risks and Mitigations

- Risk: marker assertions become brittle.
  - Mitigation: assert stable phrases tied to API intent names.
- Risk: docs edit accidentally changes behavior.
  - Mitigation: docs-only line additions plus scoped compile/test matrix.

## Interfaces and Contracts

- Guard:
  `bash scripts/dev/test-split-module-rustdoc.sh`
- Compile:
  `cargo check -p tau-github-issues-runtime --target-dir target-fast`
- Targeted tests:
  `cargo test -p tau-github-issues-runtime github_issues_runtime --target-dir target-fast`

## ADR References

- Not required.
