# Spec #2315

Status: Implemented
Milestone: specs/milestones/m50/index.md
Issue: https://github.com/njfio/Tau/issues/2315

## Problem Statement

Previously circulated "critical gap" claims are driving implementation priorities, but current code already contains features that some claims say are missing. Without a repeatable validation pass, roadmap decisions can diverge from reality and waste delivery cycles.

## Scope

In scope:

- Add a repeatable verification script that executes targeted tests for four critical claims:
  - per-session cost tracking
  - token preflight blocking
  - prompt caching wiring
  - PPO/GAE production call path
- Update `tasks/resolution-roadmap.md` with a dated critical-gap revalidation section that classifies each claim as resolved/partial/open and cites executable evidence.
- Keep changes bounded to validation tooling and documentation synchronization.

Out of scope:

- Replacing the roadmap document wholesale.
- Implementing new provider/runtime functionality not required by this verification slice.

## Acceptance Criteria

- AC-1: Given a local checkout, when running the new critical-gap verification script, then it executes all mapped validation tests and fails fast on any failing test.
- AC-2: Given the updated roadmap file, when reviewing the critical-gap section, then each of the four claims is classified with dated status and includes test evidence references.
- AC-3: Given current code on this branch, when running the verification script, then all mapped tests pass and the script exits successfully.

## Conformance Cases

- C-01 (AC-1, functional): `scripts/dev/verify-critical-gaps.sh` exists, is executable, and runs all mapped commands.
- C-02 (AC-2, conformance): `tasks/resolution-roadmap.md` contains a "Critical Gap Revalidation (2026-02-17)" section with four claim rows and evidence.
- C-03 (AC-3, integration): Running `scripts/dev/verify-critical-gaps.sh` exits `0` on the current branch.

## Success Metrics / Observable Signals

- One command reproduces critical-gap validation.
- Roadmap claim status is anchored to tests instead of anecdotal snapshots.
- Team can gate future "gap" discussions on executable evidence.
