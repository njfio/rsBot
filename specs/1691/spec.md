# Issue 1691 Spec

Status: Implemented

Issue: `#1691`  
Milestone: `#23`  
Parent: `#1623`

## Problem Statement

`tau-onboarding` contains startup, preflight, and transport-mode orchestration
paths but many split modules lack top-level `//!` docs describing phase
contracts, wizard invariants, and failure modes. This slows onboarding/runtime
debugging work.

## Scope

In scope:

- add module-level `//!` docs across onboarding startup/transport modules
- document startup phase boundaries and dispatch contracts
- document onboarding wizard/profile invariants and failure modes

Out of scope:

- behavioral onboarding logic changes
- CLI interface changes

## Acceptance Criteria

AC-1 (startup phase docs):
Given startup modules,
when files are inspected,
then `//!` docs describe preflight/resolution/dispatch boundaries.

AC-2 (wizard/profile invariants):
Given onboarding/profile modules,
when docs are read,
then invariants and state persistence expectations are explicit.

AC-3 (transport mode failure notes):
Given transport mode modules,
when docs are read,
then failure/diagnostic semantics are documented.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given startup modules, when scanning headers, then `//!` phase docs are present. |
| C-02 | AC-2 | Functional | Given onboarding/profile modules, when scanning headers, then invariants are documented. |
| C-03 | AC-3 | Conformance | Given transport-mode modules, when scanning headers, then failure/diagnostic expectations are documented. |
| C-04 | AC-1..AC-3 | Regression | Given targeted onboarding tests/docs checks, when run, then no regression is introduced. |

## Success Metrics

- onboarding startup and transport files have explicit module boundary docs
- operator/debugging entry points are easier to identify by file header
