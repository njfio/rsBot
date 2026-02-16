# Issue 1687 Spec

Status: Implemented

Issue: `#1687`  
Milestone: `#21`  
Parent: `#1638`

## Problem Statement

`crates/tau-multi-channel/src/multi_channel_runtime.rs` combines ingress loading
and normalization, route/health report orchestration, and outbound/retry helper
logic in one monolithic file, making behavior boundaries hard to maintain.

## Scope

In scope:

- extract ingress normalization/loading helpers to dedicated module(s)
- extract routing/health/orchestration helpers to dedicated module(s)
- extract outbound/retry helper logic to dedicated module(s)
- preserve runtime behavior and telemetry contracts

Out of scope:

- transport behavior changes
- command semantics changes
- dependency changes

## Acceptance Criteria

AC-1 (ingress modularization):
Given ingress helper responsibilities,
when reviewing module layout,
then ingress normalization/loading helpers are implemented in dedicated
`multi_channel_runtime` module file(s).

AC-2 (routing modularization):
Given route/health report helper responsibilities,
when reviewing module layout,
then routing orchestration helpers are implemented in dedicated
`multi_channel_runtime` module file(s).

AC-3 (outbound modularization):
Given outbound retry/delivery logging helper responsibilities,
when reviewing module layout,
then outbound/retry helpers are implemented in dedicated
`multi_channel_runtime` module file(s).

AC-4 (behavior parity):
Given existing `tau-multi-channel` runtime tests,
when running scoped checks,
then transport semantics and telemetry contracts remain unchanged.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given source layout, when inspected, then ingress helpers are moved under `crates/tau-multi-channel/src/multi_channel_runtime/ingress.rs`. |
| C-02 | AC-2 | Functional | Given source layout, when inspected, then routing helpers are moved under `crates/tau-multi-channel/src/multi_channel_runtime/routing.rs`. |
| C-03 | AC-3 | Functional | Given source layout, when inspected, then outbound/retry helpers are moved under `crates/tau-multi-channel/src/multi_channel_runtime/outbound.rs`. |
| C-04 | AC-4 | Regression | Given scoped checks, when running `tau-multi-channel` tests + strict clippy + fmt + split harness, then all pass without behavior drift. |

## Success Metrics

- root runtime file reduced to composition/orchestration surface
- ingress/routing/outbound helper domains are explicitly separated
- `tau-multi-channel` tests pass unchanged
