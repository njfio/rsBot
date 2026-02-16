# Issue 1994 Spec

Status: Implemented

Issue: `#1994`  
Milestone: `#24`  
Parent: `#1662`

## Problem Statement

Combined summary gate manifest reports can be built in-memory, but there is no
deterministic export and replay-validation helper pair for persisting and
verifying those artifacts in automation workflows.

## Scope

In scope:

- add deterministic combined-manifest-report export helper
- add replay validator helper for exported combined report JSON
- enforce required top-level `manifest` and `quality` sections

Out of scope:

- CI workflow wiring
- dashboard rendering
- remote/object-store transport

## Acceptance Criteria

AC-1 (deterministic export):
Given a valid combined manifest report and output directory,
when export runs,
then deterministic path and bytes-written summary are returned.

AC-2 (validator pass path):
Given exported combined manifest report JSON,
when validator runs,
then payload is accepted with required top-level sections.

AC-3 (validator fail closed):
Given malformed or non-object combined manifest report JSON,
when validator runs,
then deterministic validation errors are returned.

AC-4 (invalid destination fail closed):
Given export destination that is a file path,
when export runs,
then deterministic directory error is returned.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given valid combined report and output directory, when export runs, then deterministic filename is written and bytes_written > 0. |
| C-02 | AC-2 | Conformance | Given exported combined report JSON, when validator runs, then payload contains `manifest` and `quality` objects. |
| C-03 | AC-3 | Unit | Given malformed/non-object JSON, when validator runs, then deterministic parse/object validation errors are returned. |
| C-04 | AC-4 | Regression | Given export destination path that is a file, when export runs, then deterministic directory error is returned. |

## Success Metrics

- combined manifest reports are persistable with one deterministic helper
- archived reports are replay-validated without ad hoc parsing
- malformed evidence is rejected fail-closed with actionable errors
