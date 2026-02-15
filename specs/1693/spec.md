# Issue 1693 Spec

Status: Implemented

Issue: `#1693`  
Milestone: `#23`  
Parent: `#1624`

## Problem Statement

`tau-gateway` and `tau-provider` expose critical API/auth surfaces, but several
split modules lack top-level `//!` docs. Missing module contracts make endpoint
semantics, auth-mode resolution, and failure reason handling harder to audit and
operate.

## Scope

In scope:

- add module-level `//!` docs to undocumented gateway/provider modules
- document gateway endpoint/schema/runtime boundaries
- document provider auth-mode decision flow and credential-store interactions
- document failure handling/reason surfaces for API/auth client modules

Out of scope:

- runtime behavior changes
- protocol/wire format changes
- dependency changes

## Acceptance Criteria

AC-1 (gateway endpoint/schema contracts):
Given `tau-gateway` modules,
when headers are inspected,
then endpoint/schema/runtime contracts are documented.

AC-2 (provider auth-mode decisions):
Given `tau-provider` auth/client modules,
when docs are read,
then auth-mode selection and credential resolution invariants are explicit.

AC-3 (failure semantics):
Given gateway/provider runtime modules,
when docs are read,
then failure/diagnostic semantics are documented.

AC-4 (regression safety):
Given scoped checks,
when tests/docs checks run,
then no regressions are introduced.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given targeted gateway/provider files, when scanning for `//!`, then no gap files remain. |
| C-02 | AC-2 | Conformance | Given provider auth/client headers, when inspected, then auth decision invariants are explicit. |
| C-03 | AC-3 | Conformance | Given gateway/provider runtime headers, when inspected, then failure semantics are explicit. |
| C-04 | AC-4 | Regression | Given crate tests/docs checks, when run, then all pass. |

## Success Metrics

- zero missing module headers in targeted gateway/provider files
- operator-facing gateway/auth contracts are identifiable from module headers
