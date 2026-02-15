# Issue 1968 Spec

Status: Implemented

Issue: `#1968`  
Milestone: `#24`  
Parent: `#1662`

## Problem Statement

Benchmark evaluation artifacts can be built in-memory, but there is no
deterministic export helper to persist them as JSON evidence files for live runs
and operational audits.

## Scope

In scope:

- add file export helper for `BenchmarkEvaluationArtifact`
- enforce deterministic filename convention
- emit export metadata (path, bytes written)
- ensure destination directory creation and deterministic failure behavior

Out of scope:

- remote/object storage
- compression/encryption pipelines
- dashboard ingestion

## Acceptance Criteria

AC-1 (deterministic export path):
Given artifact + output directory,
when export runs,
then helper writes artifact to deterministic filename and returns that path.

AC-2 (payload parity):
Given exported file,
when read and parsed,
then JSON equals `artifact.to_json_value()`.

AC-3 (nested directory support):
Given nested destination directory that does not exist,
when export runs,
then directories are created and export succeeds.

AC-4 (invalid destination fail closed):
Given destination path that is an existing file (not directory),
when export runs,
then helper fails with deterministic error message.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given a valid artifact and empty output directory, when export runs, then deterministic filename is produced and file exists. |
| C-02 | AC-2 | Conformance | Given exported artifact JSON, when parsed, then payload equals in-memory artifact JSON value. |
| C-03 | AC-3 | Integration | Given nested missing directory path, when export runs, then directory tree is created and export succeeds. |
| C-04 | AC-4 | Unit | Given destination path that points to a file, when export runs, then deterministic error is returned. |

## Success Metrics

- benchmark evaluation artifacts are persistable via one helper call
- deterministic output naming enables stable archival and replay lookup
- export metadata supports audit log linkage
