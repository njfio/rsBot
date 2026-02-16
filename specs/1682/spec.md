# Issue 1682 Spec

Status: Implemented

Issue: `#1682`  
Milestone: `#21`  
Parent: `#1636`

## Problem Statement

`crates/tau-agent-core/src/lib.rs` currently contains startup wiring, turn-loop orchestration, tool-bridge execution, and safety/memory integration helpers in a single monolithic file. The layout obscures runtime lifecycle boundaries and increases maintenance risk for core agent behavior.

## Scope

In scope:

- split `tau-agent-core/src/lib.rs` internal lifecycle helpers into focused modules:
  - startup orchestration helpers
  - turn-loop orchestration helpers
  - tool-bridge orchestration helpers
  - safety/memory integration helpers
- keep `lib.rs` as the stable entrypoint with existing public API and re-exports
- preserve runtime behavior and test outcomes

Out of scope:

- behavioral feature additions
- protocol/wire format changes
- dependency changes

## Acceptance Criteria

AC-1 (lifecycle decomposition):
Given `tau-agent-core/src/lib.rs`,
when reviewing structure,
then startup, turn-loop, tool-bridge, and safety/memory internal orchestration are extracted into dedicated modules.

AC-2 (public API stability):
Given existing `tau-agent-core` public APIs,
when compiling and running tests,
then signatures and externally visible behavior remain unchanged.

AC-3 (runtime parity):
Given turn-loop, tool execution, safety, and memory flows,
when running targeted and full crate tests,
then behavior remains unchanged.

AC-4 (regression safety):
Given scoped quality gates,
when running formatter/linter/tests,
then all pass with no warnings.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given `lib.rs` and new module files, when inspected, then lifecycle helper implementations are hosted in the four extracted modules and `lib.rs` delegates/re-exports. |
| C-02 | AC-2 | Conformance | Given existing public API call sites, when compiling `tau-agent-core` tests, then no signature changes are required. |
| C-03 | AC-3 | Integration | Given runtime loop + tool bridge + safety/memory tests, when run, then existing flows pass unchanged. |
| C-04 | AC-4 | Regression | Given strict checks, when running `cargo test -p tau-agent-core`, strict clippy, and fmt, then all pass. |

## Success Metrics

- reduced `lib.rs` line count with clear lifecycle module boundaries
- zero public API breaks
- full `tau-agent-core` test suite remains green
