# Spec #2058

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2058

## Problem Statement

`crates/tau-cli/src/cli_args.rs` remains above the M25 target threshold and
needs a concrete, reviewable split map before code extraction begins.
Currently there is no formal artifact documenting module boundaries, ownership,
public API impact, or test migration sequencing.

## Acceptance Criteria

- AC-1: A split-map artifact defines extraction phases, module boundaries, and
  ownership for reducing `cli_args.rs` toward `<3000` LOC.
- AC-2: Import/public API impact is documented, including preserved `Cli`
  external contract and proposed flatten module boundaries.
- AC-3: Test migration plan is explicitly documented before any field/code
  moves.

## Scope

In:

- Create machine-readable and markdown split-map artifacts for `cli_args.rs`.
- Define owners and extraction groups with estimated line reductions.
- Document import/API impact and pre-move test migration sequence.

Out:

- Performing the actual field/module extraction moves (handled in `#2059`).
- Changing runtime behavior or clap flag semantics.

## Conformance Cases

- C-01 (AC-1, functional): split-map generator outputs JSON + Markdown with
  target LOC, current LOC, extraction phases, and ownership metadata.
- C-02 (AC-2, integration): split-map artifact includes explicit public API and
  import impact sections.
- C-03 (AC-3, regression): split-map tests fail closed for missing source file
  or invalid target threshold and verify test migration steps are present.

## Success Metrics

- Maintainers can execute split-map generation deterministically and review a
  single artifact for extraction sequencing before `#2059`.
