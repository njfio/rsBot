# Spec: Issue #3016 - Add CONTRIBUTING.md and SECURITY.md with conformance checks

Status: Implemented

## Problem Statement
The repository does not currently include root `CONTRIBUTING.md` and `SECURITY.md`, leaving contributor workflow and security disclosure expectations implicit.

## Acceptance Criteria

### AC-1 Root CONTRIBUTING.md exists with required workflow sections
Given the repository root,
When opening `CONTRIBUTING.md`,
Then it includes sections covering setup/prerequisites, development workflow, test/quality gates, and pull-request expectations.

### AC-2 Root SECURITY.md exists with reporting and disclosure policy
Given the repository root,
When opening `SECURITY.md`,
Then it includes vulnerability reporting channel instructions, response expectations, and coordinated disclosure guidance.

### AC-3 Conformance script enforces required doc markers
Given a deterministic shell conformance script,
When required files/sections are missing,
Then it fails;
And when files/sections are present,
Then it passes.

### AC-4 Baseline checks pass
Given the docs and script changes,
When running baseline checks,
Then `cargo fmt --check` and `cargo check -q` pass.

## Scope

### In Scope
- `CONTRIBUTING.md`
- `SECURITY.md`
- `scripts/dev/test-contributor-security-docs.sh`
- `specs/milestones/m182/index.md`
- `specs/3016/*`

### Out of Scope
- CI workflow changes.
- Security tooling implementation changes.

## Conformance Cases
- C-01: `CONTRIBUTING.md` present with required section markers.
- C-02: `SECURITY.md` present with required section markers.
- C-03: `scripts/dev/test-contributor-security-docs.sh` fails on missing sections and passes after docs are added.
- C-04: `cargo fmt --check` and `cargo check -q` pass.

## Success Metrics / Observable Signals
- `bash scripts/dev/test-contributor-security-docs.sh`
- `cargo fmt --check`
- `cargo check -q`

## Approval Gate
P2 scope: agent-authored spec, self-reviewed, implementation proceeds with human review in PR.
