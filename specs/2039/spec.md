# Spec #2039

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2039

## Problem Statement

Roadmap status visibility currently depends on inline markdown sections only.
To satisfy operational visibility requirements, the project needs documented,
deterministic roadmap status artifacts with an explicit manual and scheduled
execution path.

## Acceptance Criteria

- AC-1: Artifact generation command and JSON schema are documented in
  `docs/guides/roadmap-status-sync.md`.
- AC-2: A scheduled/manual GitHub Actions path produces roadmap status
  artifacts and publishes them as workflow artifacts.
- AC-3: Contract tests verify the workflow path and documentation references
  remain intact.

## Scope

In:

- Documentation updates for command, schema, and usage.
- Workflow for scheduled + manual artifact generation.
- Contract tests that guard docs/workflow drift.

Out:

- Dashboard ingestion of roadmap status artifacts.
- Editing milestone/issue generated status blocks beyond current sync flow.

## Conformance Cases

- C-01 (AC-1, integration): roadmap sync guide references artifact command and
  schema path.
- C-02 (AC-2, integration): workflow contains `schedule` and
  `workflow_dispatch`, runs artifact generator, and uploads JSON/Markdown
  outputs.
- C-03 (AC-3, regression): contract tests fail when docs/workflow references or
  required steps are removed.

## Success Metrics

- Operators can trigger deterministic artifact generation manually or by
  schedule without editing docs directly.
- Contract checks prevent silent removal of roadmap artifact publication path.
