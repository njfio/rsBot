# Spec and Branch Archive Operations

Run all commands from repository root.

## Implemented Spec Archive

Generate deterministic implemented-spec archive artifacts:

```bash
scripts/dev/spec-archive-index.sh
```

Default outputs:

- `tasks/reports/spec-archive-index.json`
- `tasks/reports/spec-archive-index.md`

Deterministic timestamp mode:

```bash
scripts/dev/spec-archive-index.sh --generated-at 2026-02-21T00:00:00Z
```

## Merged Branch Archive Workflow

Use stale-merged-branch reporting/prune tooling for merged branch archival hygiene:

```bash
scripts/dev/stale-merged-branch-prune.sh
```

To apply deletions only after reviewing dry-run artifacts:

```bash
scripts/dev/stale-merged-branch-prune.sh --execute --confirm-delete
```

Review produced artifacts:

- `tasks/reports/stale-merged-branch-prune.json`
- `tasks/reports/stale-merged-branch-prune.md`

## Operational Cadence

1. Run `spec-archive-index.sh` after each merged milestone slice.
2. Run stale-merged-branch prune in dry-run mode weekly.
3. Execute branch deletion only after human review of dry-run report.
