# Issue 1768 Plan

Status: Reviewed

## Approach

1. Add publication policy JSON defining:
   - snapshot naming convention
   - artifact filenames
   - retention window defaults
2. Add `scripts/dev/hierarchy-graph-publish.sh` to:
   - read current graph outputs from `#1767`
   - publish timestamped snapshots into a history directory
   - maintain discoverability index metadata
   - prune expired snapshots by retention window
3. Add contract tests for policy/script behavior and retention pruning.
4. Update roadmap operator docs to include extract + publish workflow.

## Affected Areas

- `tasks/policies/hierarchy-graph-publication-policy.json` (new)
- `scripts/dev/hierarchy-graph-publish.sh` (new)
- `.github/scripts/test_hierarchy_graph_publication_contract.py` (new)
- `docs/guides/roadmap-status-sync.md`
- `docs/README.md`

## Output Contracts

Publication policy JSON includes:

- `schema_version`, `policy_id`
- naming convention fields (`artifact_basename`, `snapshot_dir_pattern`)
- artifact filenames
- retention window (`retention_days`)

History index JSON includes:

- `schema_version`, `policy_id`, `retention_days`
- `snapshots[]` entries with snapshot id, timestamp, root issue, and artifact
  relative paths

## Risks And Mitigations

- Risk: retention pruning could remove recent artifacts unintentionally
  - Mitigation: deterministic tests with fixed `--now-utc` and explicit cutoff
    assertions.
- Risk: publication format drift can break downstream scripts
  - Mitigation: contract tests lock index schema and path conventions.

## ADR

No new dependency or protocol changes. ADR not required.
