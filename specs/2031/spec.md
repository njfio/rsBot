# Spec #2031

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2031

## Problem Statement

Roadmap source-of-truth documents drifted from live GitHub issue closure state,
and there was no deterministic artifact generation path for operational
reporting.

## Acceptance Criteria

- AC-1: Drift between generated roadmap status blocks and GitHub issue state is
  eliminated.
- AC-2: Regression checks fail fast when roadmap status artifacts are stale or
  contract references are removed.
- AC-3: Status artifacts are reproducible and auditable via deterministic
  generator + schema + workflow path.

## Scope

In:

- Sync and check generated status blocks in roadmap docs.
- Enforce guardrails in tests/workflows for drift detection.
- Publish deterministic roadmap status artifact schema/generator/workflow.

Out:

- Product feature changes outside roadmap governance operations.

## Conformance Cases

- C-01 (AC-1, functional): `scripts/dev/roadmap-status-sync.sh` updates roadmap
  status blocks and `--check --quiet` passes post-sync.
- C-02 (AC-2, regression): `scripts/dev/test-roadmap-status-sync.sh` and
  `.github/scripts/test_roadmap_status_workflow_contract.py` enforce drift-check
  guardrails and workflow integration.
- C-03 (AC-3, integration): `scripts/dev/roadmap-status-artifact.sh` plus
  `.github/scripts/test_roadmap_status_artifact_contract.py` enforce
  deterministic artifacts, schema contract, and scheduled/manual workflow path.

## Success Metrics

- Roadmap sync check passes in both local and CI paths.
- Deterministic roadmap artifacts are reproducible from fixture inputs.
- Artifact schema and workflow contract remain pinned by tests.
