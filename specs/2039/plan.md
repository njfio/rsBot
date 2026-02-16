# Plan #2039

Status: Implemented
Spec: specs/2039/spec.md

## Approach

Wire deterministic artifact generation into docs and workflow contracts:

1. Extend roadmap sync guide with artifact schema/command details.
2. Add scheduled + manual workflow that runs artifact generator in live mode
   and uploads outputs.
3. Add contract tests to lock workflow and docs references.

## Affected Modules

- `docs/guides/roadmap-status-sync.md`
- `.github/workflows/roadmap-status-artifacts.yml`
- `.github/scripts/test_roadmap_status_artifact_contract.py`
- `scripts/dev/test-roadmap-status-artifact.sh` (execution evidence)

## Risks and Mitigations

- Risk: Workflow path drifts from command/schema contract.
  - Mitigation: explicit contract tests over workflow snippets + docs references.
- Risk: Non-deterministic timestamps make diff-based audits noisy.
  - Mitigation: generator accepts fixed timestamp overrides; workflow captures
    generated files as artifacts instead of committing them.

## Interfaces and Contracts

- Workflow contract:
  `.github/workflows/roadmap-status-artifacts.yml` with `schedule` +
  `workflow_dispatch`.
- Guide contract:
  `docs/guides/roadmap-status-sync.md` includes command, schema, and workflow
  references.

## ADR References

- Not required.
