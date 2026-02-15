# Issue 1744 Spec

Status: Implemented

Issue: `#1744`  
Milestone: `#24`  
Parent: `#1662`

## Problem Statement

Runtime lifecycle controls need structured, machine-validated audit logs so
operators can verify every control transition and detect malformed or
non-compliant audit records.

## Scope

In scope:

- define a stable lifecycle control audit record schema in `tau-runtime`
- emit lifecycle control audit records during RPC serve control transitions
- add diagnostics compliance checks for lifecycle audit records
- add tests for unit/functional/regression coverage

Out of scope:

- introducing new RPC wire-format frame kinds
- changing gateway control APIs
- adding new external dependencies

## Acceptance Criteria

AC-1 (schema):
Given lifecycle control audit logging,
when records are emitted,
then each record conforms to a documented v1 schema with stable fields.

AC-2 (transition logging):
Given serve-mode control transitions,
when start/cancel lifecycle controls are processed,
then structured lifecycle audit records are emitted for each accepted control.

AC-3 (compliance assertions):
Given audit files containing lifecycle control records,
when diagnostics summarizes the file,
then compliant lifecycle records are counted and non-compliant records are
reported deterministically.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Unit | Given lifecycle control records, when schema validation runs, then required fields and allowed control actions are enforced. |
| C-02 | AC-2 | Functional | Given serve input with run.start and run.cancel, when serving with lifecycle audit enabled, then one audit record per control transition is emitted with expected action/state fields. |
| C-03 | AC-3 | Regression | Given malformed lifecycle control audit records, when diagnostics summarizes the file, then compliance failure counters increment and valid counters remain accurate. |

## Success Metrics

- lifecycle control transitions become auditable with deterministic schema
  validation
- diagnostics provides explicit compliance counters for lifecycle audit health
