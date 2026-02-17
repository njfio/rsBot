# Session Cost Mutation Lane

This runbook defines the deterministic mutation lane for the session usage/cost conformance slice (C-01..C-04).

## Canonical Invocation

1. Set a deterministic diff path (or rely on the default).

```bash
export SESSION_COST_DIFF_PATH=/tmp/session-cost-mutation.diff
```

2. Execute the canonical QA loop config.

```bash
cargo run -p tau-coding-agent -- /qa-loop --config docs/qa/session-cost-mutation.qa-loop.json --json
```

## Notes

- The config scopes mutation runs to session-cost files and tests only.
- Mutation stages use `--baseline skip` because baseline conformance is covered by the preceding `baseline-c01..c04` stages.
- All stages use an isolated target directory: `CARGO_TARGET_DIR=target-fast-2379`.
