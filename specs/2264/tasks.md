# Tasks #2264

Status: In Progress
Spec: specs/2264/spec.md
Plan: specs/2264/plan.md

- T1 (tests first): add RED conformance tests/scripts for C-01..C-04
  (formula structure, platform checksum mapping, release workflow publish path).
- T2: implement formula renderer script and fixture-driven generation logic.
- T3: add formula validation script and make RED tests pass.
- T4: wire release workflow to render and upload Homebrew formula asset.
- T5: extend release workflow contract tests for formula render/publish steps.
- T6: update docs (`README.md`, `docs/guides/release-automation-ops.md`) for
  Homebrew install/upgrade/uninstall usage.
- T7: run scoped verification and collect PR evidence:
  - `cargo fmt --check`
  - release helper/contract tests
  - formula render/validation scripts.
