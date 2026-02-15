# Consolidated Runtime Rollback Drill

Use this runbook to execute a deterministic rollback simulation for consolidated runtime surfaces.

## Trigger conditions

Rollback is required when any high/medium trigger is active.

| Trigger ID | Severity | Condition |
| --- | --- | --- |
| `proof-summary-missing` | high | Retained-capability proof summary artifact is missing. |
| `proof-runs-failed` | high | `failed_runs > 0` in retained-capability proof summary. |
| `proof-markers-missing` | high | Any run reports missing output/artifact markers. |
| `validation-matrix-missing` | medium | Validation matrix artifact is missing. |
| `validation-open-issues` | medium | Validation matrix still reports open tracked issues. |
| `validation-completion-below-100` | low | Validation matrix completion percent is below 100%. |

## Step-by-step rollback drill

1. Freeze promotion and generate rollback checklist artifacts:
   ```bash
   ./scripts/demo/rollback-drill-checklist.sh
   ```
2. Archive required rollback evidence before any revert:
   - retained-capability proof summary (JSON + Markdown)
   - proof logs + generated proof artifacts
   - validation matrix JSON/Markdown
3. Execute bounded rollback:
   ```bash
   git revert <commit-sha>
   ./scripts/dev/m21-retained-capability-proof-summary.sh --binary ./target/debug/tau-coding-agent
   ```
4. Validate rollback exit criteria:
   - proof summary reports `failed_runs == 0`
   - proof marker missing count is `0`
   - no active high/medium rollback triggers remain

## Artifact capture checklist

`scripts/demo/rollback-drill-checklist.sh` emits:

- `tasks/reports/m21-rollback-drill-checklist.json`
- `tasks/reports/m21-rollback-drill-checklist.md`

Checklist artifacts include:

- proof summary JSON path + presence status
- validation matrix JSON path + presence status
- proof Markdown/log/artifact directory pointers (when present)

## Deterministic simulation commands

Generate retained-capability proof artifacts:

```bash
./scripts/dev/m21-retained-capability-proof-summary.sh \
  --binary ./target/debug/tau-coding-agent
```

Generate rollback drill report and fail closed when rollback triggers are active:

```bash
./scripts/demo/rollback-drill-checklist.sh --fail-on-trigger
```

## Ownership

Primary ownership surfaces:
- `scripts/demo/rollback-drill-checklist.sh` (rollback trigger + checklist generation)
- `scripts/dev/m21-retained-capability-proof-summary.sh` (proof-run input artifacts)
- `docs/guides/runbook-ownership-map.md` (consolidated runbook ownership map)

Ownership map: `docs/guides/runbook-ownership-map.md`.
