# Memory Operations Runbook

Run all commands from repository root.

## Scope

`--memory-contract-runner` is removed. Runtime memory behavior is owned by `tau-agent-core`.
`tau-memory` now owns only fixture schemas and replay helpers for contract validation.

## Health and observability signals

Primary status signal:

```bash
cargo run -p tau-coding-agent -- \
  --memory-state-dir .tau/memory \
  --transport-health-inspect memory \
  --transport-health-json
```

Primary state files:

- `.tau/memory/state.json`
- Optional historical artifact logs under `.tau/memory/*.jsonl` if produced by older revisions.

## Deterministic demo path

```bash
./scripts/demo/memory.sh
```

## Rollout plan with guardrails

1. Validate memory fixture contracts (`tau-memory`):
   `cargo test -p tau-memory memory_contract -- --test-threads=1`
2. Validate runtime memory behavior (`tau-agent-core`):
   `cargo test -p tau-agent-core memory -- --test-threads=1`
3. Run deterministic diagnostics demo:
   `./scripts/demo/memory.sh`
4. Confirm health snapshot is `healthy` before promotion:
   `--transport-health-inspect memory --transport-health-json`
5. Promote while monitoring:
   `failure_streak`, `last_cycle_failed`, and `queue_depth`.

## Rollback plan

1. Do not invoke `--memory-contract-runner` (removed).
2. Preserve `.tau/memory/` for incident analysis.
3. Revert to last known-good revision:
   `git revert <commit>`
4. Re-run validation matrix before re-enable.

## Troubleshooting

- Symptom: memory diagnostics fail to load state.
  Action: verify `.tau/memory/state.json` exists and contains a `health` object.
- Symptom: runtime recall quality regresses.
  Action: run `cargo test -p tau-agent-core memory -- --test-threads=1` and inspect embedding/retrieval test failures.
- Symptom: health state `failing` (`failure_streak >= 3`).
  Action: treat as rollout gate failure; pause promotion and investigate repeated runtime failures in active memory producers.
- Symptom: non-zero `queue_depth`.
  Action: investigate upstream producers writing memory health artifacts and reduce backlog before promotion.
