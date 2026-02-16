# Spec #2064

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2064

## Problem Statement

`crates/tau-github-issues-runtime/src/github_issues_runtime.rs` remains above
the M25 threshold and lacks a deterministic split-map artifact defining phased
extraction boundaries, ownership, API/import impact, and test migration
sequence before code movement.

## Acceptance Criteria

- AC-1: A deterministic split-map artifact documents phased extraction
  boundaries and owners for reducing `github_issues_runtime.rs` below 3000 LOC.
- AC-2: Public API/import impact is documented, including stable runtime entry
  points for the GitHub Issues bridge.
- AC-3: Test migration plan is explicitly documented before extraction begins.

## Scope

In:

- Generate JSON + Markdown split-map artifacts for GitHub Issues runtime
  decomposition.
- Define phased module boundaries and estimated extraction by domain.
- Document API/import impact and test migration sequencing.

Out:

- Executing the extraction move itself (`#2065`).

## Conformance Cases

- C-01 (AC-1, functional): split-map generator emits deterministic JSON +
  Markdown artifacts with target/current LOC and extraction phases.
- C-02 (AC-2, integration): split-map includes non-empty public API impact and
  import impact sections.
- C-03 (AC-3, regression): split-map tests fail closed on missing source file
  or invalid target threshold and require non-empty migration steps.

## Success Metrics

- Maintainers have executable, test-backed split-map artifacts before runtime
  code extraction starts.
