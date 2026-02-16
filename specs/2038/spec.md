# Spec #2038

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2038

## Problem Statement

Roadmap status drift checks must be continuously enforced in both local and CI
flows to prevent stale generated blocks from being merged.

## Acceptance Criteria

- AC-1: Roadmap sync regression harness exists and passes in local checks.
- AC-2: Workflow contract tests assert CI/docs workflows run roadmap drift checks.
- AC-3: Drift check command fails when status blocks diverge and passes when in sync.

## Scope

In:

- Validate existing roadmap sync guardrail tests and workflow contracts.
- Confirm check command execution in quiet strict mode.

Out:

- New workflow creation.

## Conformance Cases

- C-01 (AC-1): `scripts/dev/test-roadmap-status-sync.sh` passes.
- C-02 (AC-2): `.github/scripts/test_roadmap_status_workflow_contract.py` passes.
- C-03 (AC-3): `scripts/dev/roadmap-status-sync.sh --check --quiet` passes when synced.

## Success Metrics

- Guardrail tests remain green in local/CI execution paths.
