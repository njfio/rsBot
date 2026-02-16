# Spec #2052

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2052

## Problem Statement

Namespace and hierarchy validation must be continuously enforced by local/CI
gates; otherwise metadata drift can reappear despite policy updates.

## Acceptance Criteria

- AC-1: CI/local validation paths execute updated drift-check tests.
- AC-2: Policy and docs are synchronized with namespace/hierarchy enforcement.
- AC-3: Regression suite remains green after integration.

## Scope

In:

- Wire updated drift policy behavior through existing test/CI paths.
- Update docs describing required namespaces and condition IDs.

Out:

- New CI workflows.

## Conformance Cases

- C-01 (AC-1): `scripts/dev/test-dependency-drift-check.sh` passes.
- C-02 (AC-2): policy contract tests and docs condition-ID references pass.
- C-03 (AC-3): full `.github/scripts` regression suite passes.

## Success Metrics

- Existing CI paths that execute script/policy tests remain green with updated contract.
