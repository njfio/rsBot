# Spec #2062

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2062

## Problem Statement

`crates/tau-tools/src/tools.rs` is above the M25 threshold and lacks a formal
split-map artifact defining phased extraction boundaries, ownership, API/import
impact, and test migration sequencing before code movement.

## Acceptance Criteria

- AC-1: A deterministic split-map artifact documents phased extraction
  boundaries and owners for reducing `tools.rs` below 3000 LOC.
- AC-2: Public API/import impact is documented, including stable command/tool
  entrypoints consumed by runtime callers.
- AC-3: Test migration plan is explicitly documented before extraction starts.

## Scope

In:

- Generate JSON + Markdown split-map artifacts for `tools.rs` decomposition.
- Define module boundaries and estimated extraction volume by phase.
- Document API/import impact and test migration sequencing.

Out:

- Executing the split itself (`#2063`).

## Conformance Cases

- C-01 (AC-1, functional): split-map generator emits deterministic JSON +
  Markdown artifacts with target/current LOC and phased extraction details.
- C-02 (AC-2, integration): split-map includes non-empty public API impact and
  import impact sections.
- C-03 (AC-3, regression): split-map tests fail closed on missing source file
  or invalid target threshold and require non-empty test migration steps.

## Success Metrics

- Maintainers have executable, test-backed split-map artifacts for `tools.rs`
  before extraction work begins.
