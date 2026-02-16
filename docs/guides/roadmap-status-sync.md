# Roadmap Status Sync

This guide describes how to refresh generated roadmap status blocks in:

- `tasks/todo.md`
- `tasks/tau-vs-ironclaw-gap-list.md`

Use `scripts/dev/roadmap-status-sync.sh` to avoid manual drift between docs and GitHub issue state.

Tracked roadmap groups/IDs are configured in `tasks/roadmap-status-config.json`.

Hierarchy drift/orphan detection rules are defined in:

- `tasks/policies/issue-hierarchy-drift-rules.json`
- `docs/guides/issue-hierarchy-drift-rules.md`

PR batch lane ownership boundaries are defined in:

- `tasks/policies/pr-batch-lane-boundaries.json`
- `tasks/policies/pr-batch-exceptions.json`
- `docs/guides/pr-batch-lane-boundaries.md`

Stale-branch threshold and alert rules are defined in:

- `tasks/policies/stale-branch-alert-policy.json`
- `docs/guides/stale-branch-response-playbook.md`

Hierarchy graph extraction for the `#1678` execution tree:

- `scripts/dev/hierarchy-graph-extractor.sh`

Hierarchy graph publication and retention policy:

- `scripts/dev/hierarchy-graph-publish.sh`
- `tasks/policies/hierarchy-graph-publication-policy.json`

Critical-path update template and risk rubric:

- `tasks/templates/critical-path-update-template.md`
- `tasks/policies/critical-path-risk-rubric.json`

Critical-path cadence enforcement policy and checklist:

- `tasks/policies/critical-path-update-cadence-policy.json`
- `tasks/templates/critical-path-cadence-checklist.md`
- `scripts/dev/critical-path-cadence-check.sh`

Roadmap status artifact generator + schema + workflow:

- `scripts/dev/roadmap-status-artifact.sh`
- `tasks/schemas/roadmap-status-artifact.schema.json`
- `.github/workflows/roadmap-status-artifacts.yml`

To preview hierarchy drift findings locally before CI:

```bash
scripts/dev/dependency-drift-check.sh --mode dry-run
```

To generate machine-readable + Markdown hierarchy artifacts:

```bash
scripts/dev/hierarchy-graph-extractor.sh \
  --root-issue 1678 \
  --output-json tasks/reports/issue-hierarchy-graph.json \
  --output-md tasks/reports/issue-hierarchy-graph.md
```

To publish a timestamped history snapshot and enforce retention:

```bash
scripts/dev/hierarchy-graph-publish.sh \
  --graph-json tasks/reports/issue-hierarchy-graph.json \
  --graph-md tasks/reports/issue-hierarchy-graph.md
```

Published history artifacts are discoverable via:

- `tasks/reports/issue-hierarchy-history/index.json`
- `tasks/reports/issue-hierarchy-history/index.md`

To publish recurring critical-path updates in tracker comments, copy the
canonical template and fill per-lane status/risk fields:

```bash
cat tasks/templates/critical-path-update-template.md
```

To validate update cadence and escalation status before publishing:

```bash
scripts/dev/critical-path-cadence-check.sh --json
```

## Prerequisites

- `gh` authenticated for this repository.
- `jq` and `python3` available.

## Refresh Status Blocks

```bash
scripts/dev/roadmap-status-sync.sh
```

The command queries tracked roadmap issues and rewrites only the generated marker blocks:

- `<!-- ROADMAP_STATUS:BEGIN --> ... <!-- ROADMAP_STATUS:END -->`
- `<!-- ROADMAP_GAP_STATUS:BEGIN --> ... <!-- ROADMAP_GAP_STATUS:END -->`

## Check Mode (CI / Pre-Commit)

```bash
scripts/dev/roadmap-status-sync.sh --check
```

`--check` exits non-zero when either generated block is stale and prints a diff.

Use `--quiet` to suppress informational success output in CI:

```bash
scripts/dev/roadmap-status-sync.sh --check --quiet
```

## Fixture Mode (Deterministic Tests)

```bash
scripts/dev/roadmap-status-sync.sh \
  --fixture-json path/to/fixture.json \
  --config-path tasks/roadmap-status-config.json \
  --todo-path /tmp/todo.md \
  --gap-path /tmp/gap.md
```

Fixture format:

```json
{
  "default_state": "OPEN",
  "issues": [
    { "number": 1425, "state": "CLOSED" }
  ]
}
```

## Script Tests

```bash
scripts/dev/test-roadmap-status-sync.sh
python3 -m unittest discover -s .github/scripts -p "test_hierarchy_graph_extractor_contract.py"
python3 -m unittest discover -s .github/scripts -p "test_hierarchy_graph_publication_contract.py"
python3 -m unittest discover -s .github/scripts -p "test_critical_path_update_template_contract.py"
python3 -m unittest discover -s .github/scripts -p "test_critical_path_cadence_policy_contract.py"
```
