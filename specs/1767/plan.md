# Issue 1767 Plan

Status: Reviewed

## Approach

1. Add `scripts/dev/hierarchy-graph-extractor.sh` with:
   - live GitHub API collection (`gh api`) and bounded retries
   - fixture mode for deterministic testing
   - output contract for JSON and Markdown artifacts
2. Add extractor contract tests under `.github/scripts/` using fixture-driven
   subprocess execution.
3. Update roadmap operator docs to include extractor usage and test command.

## Affected Areas

- `scripts/dev/hierarchy-graph-extractor.sh` (new)
- `.github/scripts/test_hierarchy_graph_extractor_contract.py` (new)
- `docs/guides/roadmap-status-sync.md`
- `docs/README.md`

## Output Contracts

JSON output:

- `schema_version`
- `generated_at`
- `repository`
- `root_issue_number`
- `nodes[]` (normalized issue records)
- `edges[]` (parent-child edges)
- `missing_links[]`
- `orphan_nodes[]`
- `summary`

Markdown output:

- heading + generation metadata
- hierarchy tree rooted at target issue
- missing links section
- orphan nodes section

## Risks And Mitigations

- Risk: API pagination/retry flakiness
  - Mitigation: bounded retries with exponential backoff and deterministic
    fixture mode for CI.
- Risk: graph drift due schema changes in issue payload
  - Mitigation: contract tests validate required output fields and anomaly
    behavior.

## ADR

No new dependency or protocol change. ADR not required.
