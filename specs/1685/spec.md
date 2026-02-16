# Issue 1685 Spec

Status: Implemented

Issue: `#1685`  
Milestone: `#21`  
Parent: `#1637`

## Problem Statement

`crates/tau-runtime/src/rpc_protocol_runtime.rs` currently combines parsing,
dispatch semantics, NDJSON transport loops, serve-session lifecycle state, and
compatibility fixtures in one file. The monolithic layout makes behavior
boundaries hard to review and slows safe changes.

## Scope

In scope:

- split protocol parsing into dedicated module(s)
- split frame dispatch and lifecycle transition handling into dedicated module(s)
- split NDJSON dispatch/serve transport orchestration into dedicated module(s)
- preserve wire behavior, error-code contracts, and schema compatibility

Out of scope:

- protocol/schema changes
- new frame kinds or transport modes
- dependency changes

## Acceptance Criteria

AC-1 (parser modularization):
Given RPC frame schema parsing and validation logic,
when reviewing module layout,
then parsing responsibilities are implemented in dedicated parsing module(s), not
inline in the monolithic runtime file.

AC-2 (dispatch modularization):
Given frame-kind dispatch and lifecycle transition logic,
when reviewing module layout,
then dispatch responsibilities are implemented in dedicated dispatch module(s),
including run lifecycle state handling.

AC-3 (transport modularization):
Given NDJSON input/serve loops,
when reviewing module layout,
then transport orchestration is implemented in dedicated transport module(s)
while preserving response ordering and error counting.

AC-4 (behavior parity):
Given existing RPC compatibility fixtures and crate tests,
when running scoped quality gates,
then RPC behavior and error contract outputs remain unchanged.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given source layout, when inspected, then parsing logic is hosted under `crates/tau-runtime/src/rpc_protocol_runtime/` parsing module file(s). |
| C-02 | AC-2 | Functional | Given source layout, when inspected, then dispatch and lifecycle state logic is hosted under `crates/tau-runtime/src/rpc_protocol_runtime/` dispatch module file(s). |
| C-03 | AC-3 | Integration | Given mixed NDJSON transport inputs, when running `tau-runtime` tests, then dispatch/serve reports and output ordering remain stable. |
| C-04 | AC-4 | Regression | Given schema/error fixtures, when running crate tests + strict clippy + fmt + split harness, then all checks pass without behavior drift. |

## Success Metrics

- `rpc_protocol_runtime.rs` reduced to orchestration/composition surface
- parser/dispatch/transport concerns are physically separated
- `tau-runtime` tests and schema compatibility fixtures pass unchanged
