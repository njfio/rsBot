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

To preview hierarchy drift findings locally before CI:

```bash
scripts/dev/dependency-drift-check.sh --mode dry-run
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
```
