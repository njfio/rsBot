# Issue 1986 Spec

Status: Accepted

Issue: `#1986`  
Milestone: `#24`  
Parent: `#1662`

## Problem Statement

Summary gate reports are currently in-memory only. There is no deterministic
helper pair to export summary gate reports and replay-validate archived payloads
for automation/audit workflows.

## Scope

In scope:

- add deterministic summary gate report export helper
- add replay validator helper for exported summary gate report JSON
- preserve required top-level `summary` and `quality` payload sections

Out of scope:

- CI workflow wiring
- dashboard rendering
- remote/object-store transport

## Acceptance Criteria

AC-1 (deterministic export):
Given a valid summary gate report and output directory,
when export runs,
then deterministic path and bytes-written summary are returned.

AC-2 (validator pass path):
Given exported summary gate report JSON,
when validator runs,
then payload is accepted with required top-level sections.

AC-3 (validator fail closed):
Given malformed or non-object summary gate report JSON,
when validator runs,
then deterministic validation errors are returned.

AC-4 (invalid destination fail closed):
Given export destination that is a file path,
when export runs,
then deterministic directory error is returned.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given valid summary gate report and empty output directory, when export runs, then deterministic filename is written and bytes_written > 0. |
| C-02 | AC-2 | Conformance | Given exported summary gate report, when validator runs, then payload contains `summary` and `quality` objects. |
| C-03 | AC-3 | Unit | Given malformed/non-object JSON, when validator runs, then deterministic parse/object validation errors are returned. |
| C-04 | AC-4 | Regression | Given export destination path that is a file, when export runs, then deterministic directory error is returned. |

## Success Metrics

- summary gate reports can be persisted and replay-validated with deterministic
  helpers
- exported payload remains machine-readable for automation consumers
- malformed evidence is rejected fail-closed with actionable errors
