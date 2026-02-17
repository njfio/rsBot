# Tasks #2263

Status: Completed
Spec: specs/2263/spec.md
Plan: specs/2263/plan.md

- T1 (tests first): add RED conformance tests/scripts for C-01..C-04
  (Dockerfile/runtime smoke + workflow tag/publish checks).
- T2: implement Docker packaging artifacts (`Dockerfile`, `.dockerignore`) and
  make RED tests pass.
- T3: wire CI + release workflow Docker build/smoke/publish paths.
- T4: update docs (`README.md`, release guide) for container usage and tag
  semantics.
- T5: run scoped verification and collect PR evidence:
  - `cargo fmt --check`
  - targeted test/script commands for Docker packaging
  - workflow/static checks for CI/release wiring.
