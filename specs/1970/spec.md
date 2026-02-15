# Issue 1970 Spec

Status: Implemented

Issue: `#1970`  
Milestone: `#24`  
Parent: `#1662`

## Problem Statement

Benchmark artifacts can be exported, but there is no deterministic replay
validator to confirm archived files are structurally valid and schema-compatible
before downstream analysis.

## Scope

In scope:

- add exported-artifact replay validator in `tau-trainer`
- validate schema version and required top-level keys
- return parsed JSON payload when artifact passes validation

Out of scope:

- typed serde deserialization into dedicated Rust structs
- remote artifact retrieval
- dashboard integration

## Acceptance Criteria

AC-1 (valid artifact replay):
Given a valid exported benchmark artifact file,
when validator runs,
then parsed JSON payload is returned.

AC-2 (malformed JSON fail closed):
Given malformed artifact JSON,
when validator runs,
then deterministic parse error is returned.

AC-3 (required-key validation):
Given artifact missing required top-level keys,
when validator runs,
then deterministic validation error names missing key.

AC-4 (schema-version validation):
Given artifact with unsupported schema version,
when validator runs,
then deterministic unsupported-version error is returned.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given valid exported artifact, when validator runs, then parsed JSON object is returned. |
| C-02 | AC-2 | Unit | Given malformed JSON text, when validator runs, then deterministic parse error is returned. |
| C-03 | AC-3 | Conformance | Given artifact missing one required key, when validator runs, then missing-key validation error is returned. |
| C-04 | AC-4 | Integration | Given artifact with unsupported schema version, when validator runs, then schema-version error is returned. |

## Success Metrics

- archived benchmark artifacts can be replay-validated in one helper call
- deterministic errors speed triage for invalid artifacts
- schema compatibility is explicitly checked before downstream consumption
