# Issue 1743 Spec

Status: Implemented

Issue: `#1743`  
Milestone: `#24`  
Parent: `#1662`

## Problem Statement

M24 benchmark guidance exists, but report publication format and archival policy
are not standardized. Without a fixed schema and retention naming rules,
benchmark evidence is difficult to compare and audit over time.

## Scope

In scope:

- define a benchmark report publication schema/template
- define `.tau/reports` archival naming conventions
- define retention policy fields and lifecycle expectations
- add deterministic validator + regression test for report contract
- document policy in `docs/guides/training-ops.md`

Out of scope:

- benchmark fixture execution logic
- statistical significance computation internals

## Acceptance Criteria

AC-1 (report schema):
Given benchmark publication artifacts,
when validator runs,
then required report schema fields are enforced.

AC-2 (archival naming conventions):
Given report file names/paths,
when validator runs,
then naming/path patterns follow defined `.tau/reports` conventions.

AC-3 (retention policy):
Given published reports,
when reviewed/validated,
then retention metadata fields exist and are deterministic.

AC-4 (documentation):
Given `training-ops.md`,
when maintainers follow the guide,
then publication format and archival policy are reproducible.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given valid report JSON, when validator runs, then it succeeds. |
| C-02 | AC-2 | Regression | Given invalid naming/path, when validator runs, then it fails with explicit reason. |
| C-03 | AC-3 | Regression | Given missing retention fields, when validator runs, then it fails deterministically. |
| C-04 | AC-4 | Functional | Given docs update, when reviewed, then schema + archival + retention rules are documented. |

## Success Metrics

- benchmark reports are comparable and auditable across runs
