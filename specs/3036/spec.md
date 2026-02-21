# Spec: Issue #3036 - Contributor and Security policy docs hardening

Status: Implemented

## Problem Statement
Root-level contributor and security policy docs exist but are too thin for consistent process adherence. The docs should explicitly encode repository workflow expectations and security reporting/triage standards, and be discoverable from `README.md`.

## Acceptance Criteria

### AC-1 CONTRIBUTING policy is explicit and operational
Given a new contributor,
When they read `CONTRIBUTING.md`,
Then they can follow concrete setup, issue/spec workflow, test/quality gates, and PR requirements aligned with repository contracts.

### AC-2 SECURITY policy is explicit and private-by-default
Given a reporter with a potential vulnerability,
When they read `SECURITY.md`,
Then they receive private reporting channels, required submission details, response timelines, and disclosure expectations.

### AC-3 Docs discoverability is enforced
Given repository root docs,
When running docs conformance checks,
Then `README.md` links to `CONTRIBUTING.md` and `SECURITY.md`, and conformance script assertions pass.

### AC-4 Verification gates pass
Given doc updates,
When running validation,
Then docs conformance plus formatting/check commands pass without regression.

## Scope

### In Scope
- `CONTRIBUTING.md`
- `SECURITY.md`
- `README.md`
- `scripts/dev/test-docs-capability-archive.sh`
- `specs/milestones/m187/index.md`
- `specs/3036/*`

### Out of Scope
- New legal policy documents outside `CONTRIBUTING.md`/`SECURITY.md`.
- CI workflow restructuring beyond docs conformance checks.

## Conformance Cases
- C-01: CONTRIBUTING includes setup, issue/spec workflow, test gates, PR checklist.
- C-02: SECURITY includes private reporting path, required report content, triage SLA, disclosure model.
- C-03: README includes contributor/security policy links.
- C-04: Docs conformance script validates the added sections/links.
- C-05: Verification commands pass.

## Success Metrics / Observable Signals
- `scripts/dev/test-docs-capability-archive.sh`
- `cargo fmt --check`
- `cargo check -q`

## Approval Gate
P1 scope: spec authored/reviewed by agent; implementation proceeds and is flagged for human review in PR.
