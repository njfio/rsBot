# Spec #2060

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2060

## Problem Statement

`crates/tau-trainer/src/benchmark_artifact.rs` remains above the M25 threshold
and currently lacks a formal split-map artifact defining phased extraction
boundaries, ownership, API/import impact, and test migration sequence.

## Acceptance Criteria

- AC-1: A split-map artifact documents phased extraction boundaries and owners
  for reducing `benchmark_artifact.rs` toward `<3000` LOC.
- AC-2: Public API/import impact is documented, including stable interfaces for
  benchmark schema, IO, and reporting call sites.
- AC-3: Test migration plan is explicitly documented before code moves.

## Scope

In:

- Generate JSON + Markdown split-map artifacts for benchmark artifact
  decomposition planning.
- Define phased module boundaries and estimated line reductions.
- Document API/import impacts and test migration sequencing.

Out:

- Executing the extraction move itself (`#2061`).

## Conformance Cases

- C-01 (AC-1, functional): split-map generator emits deterministic JSON +
  Markdown artifacts with target/current LOC and extraction phases.
- C-02 (AC-2, integration): split-map includes non-empty public API impact and
  import impact sections.
- C-03 (AC-3, regression): split-map tests fail closed on missing source file
  or invalid target threshold and require non-empty migration steps.

## Success Metrics

- Maintainers have an executable, test-backed split-map artifact ready before
  decomposition execution begins.
