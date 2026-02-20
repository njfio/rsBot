# Plan: Issue #2778 - G23 Fly.io CI pipeline validation (optional)

## Approach
1. Capture RED evidence that current CI workflow lacks Fly validation.
2. Add scoped Fly change detection and validation steps in `.github/workflows/ci.yml`.
3. Verify workflow syntax and local conformance checks.
4. Update roadmap/spec/task evidence and close issue chain.

## Affected Modules
- `.github/workflows/ci.yml`
- `tasks/spacebot-comparison.md`
- `specs/2778/*`

## Risks and Mitigations
- Risk: workflow syntax regression.
  - Mitigation: keep changes minimal and run local workflow lint/conformance commands.
- Risk: introducing secret dependency in CI.
  - Mitigation: use `flyctl config validate` only; no deploy commands.

## Interface and Contract Notes
- CI behavior remains additive/optional.
- Fly checks run only when relevant files change (or non-PR runs where policy chooses true).
