# Plan #2031

Status: Implemented
Spec: specs/2031/spec.md

## Approach

Execute three child tasks in sequence:

1. `#2037/#2055`: regenerate roadmap status blocks from GitHub state.
2. `#2038/#2056`: validate drift guardrail tests and workflow contracts.
3. `#2039/#2057`: add deterministic artifact schema/generator/workflow and
   contract tests.

## Affected Modules

- `tasks/todo.md`
- `tasks/tau-vs-ironclaw-gap-list.md`
- `scripts/dev/roadmap-status-sync.sh`
- `scripts/dev/roadmap-status-artifact.sh`
- `scripts/dev/test-roadmap-status-sync.sh`
- `scripts/dev/test-roadmap-status-artifact.sh`
- `.github/scripts/test_roadmap_status_workflow_contract.py`
- `.github/scripts/test_roadmap_status_artifact_contract.py`
- `docs/guides/roadmap-status-sync.md`
- `tasks/schemas/roadmap-status-artifact.schema.json`
- `.github/workflows/roadmap-status-artifacts.yml`

## Risks and Mitigations

- Risk: Manual edits can reintroduce status drift.
  - Mitigation: `--check` enforcement in CI and regression harness.
- Risk: Artifact behavior can drift from documented contract.
  - Mitigation: dedicated contract tests for schema/docs/workflow.

## Interfaces and Contracts

- Roadmap status sync command contract:
  `scripts/dev/roadmap-status-sync.sh`.
- Roadmap status artifact command contract:
  `scripts/dev/roadmap-status-artifact.sh`.
- Schema contract:
  `tasks/schemas/roadmap-status-artifact.schema.json`.
- Scheduled/manual execution contract:
  `.github/workflows/roadmap-status-artifacts.yml`.

## ADR References

- Not required.
