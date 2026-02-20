# Spec: Issue #2778 - G23 Fly.io CI pipeline validation (optional)

Status: Implemented

## Problem Statement
`tasks/spacebot-comparison.md` has one remaining unchecked row: optional Fly.io CI/CD pipeline integration. The repository already includes `fly.toml` and deployment docs, but CI does not currently validate Fly manifest correctness.

## Acceptance Criteria

### AC-1 CI includes optional Fly validation path
Given GitHub Actions CI workflow definitions,
When #2778 is implemented,
Then CI includes a Fly.io manifest validation step/job with bounded scope.

### AC-2 Fly validation runs without deploy credentials
Given pull requests that touch Fly manifest/workflow files,
When CI executes,
Then validation runs using static config checks only (no deploy/action requiring secrets).

### AC-3 Existing CI quality behavior remains intact
Given current quality/coverage/smoke jobs,
When Fly validation is added,
Then existing gates continue functioning with no workflow regressions.

### AC-4 Roadmap evidence is reconciled
Given implementation completion,
When verification passes,
Then G23 optional pipeline row is checked with `#2778` evidence.

## Scope

### In Scope
- `.github/workflows/ci.yml` Fly validation integration.
- Optional-path scope detection for Fly-related changes.
- Roadmap/spec evidence updates.

### Out of Scope
- Live Fly deployment in CI.
- Secret provisioning or production rollout automation changes.

## Conformance Cases
- C-01 (conformance): workflow contains Fly validation scope and execution steps.
- C-02 (functional): Fly validation command targets `fly.toml` and requires no secrets.
- C-03 (regression): existing CI jobs still parse and run after workflow change.
- C-04 (docs): G23 row checked with `#2778` evidence.

## Success Metrics / Observable Signals
- No unchecked rows remain in `tasks/spacebot-comparison.md`.
- CI includes explicit, optional Fly validation behavior for relevant changes.

## Approval Gate
This task modified CI/CD workflow behavior and proceeded under explicit user direction to continue contract execution end-to-end.
