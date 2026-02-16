# GitHub Issues Runtime Split Map (M25)

This guide defines the pre-extraction split plan for:

- `crates/tau-github-issues-runtime/src/github_issues_runtime.rs`

Goal:

- reduce the primary file below 3000 LOC in `#2043` while preserving GitHub
  Issues bridge behavior and external runtime contracts.

## Generate Artifacts

```bash
scripts/dev/github-issues-runtime-split-map.sh
```

Default outputs:

- `tasks/reports/m25-github-issues-runtime-split-map.json`
- `tasks/reports/m25-github-issues-runtime-split-map.md`

Schema:

- `tasks/schemas/m25-github-issues-runtime-split-map.schema.json`

Deterministic replay:

```bash
scripts/dev/github-issues-runtime-split-map.sh \
  --generated-at 2026-02-16T00:00:00Z \
  --output-json /tmp/m25-github-issues-runtime-split-map.json \
  --output-md /tmp/m25-github-issues-runtime-split-map.md
```

## Validation

```bash
scripts/dev/test-github-issues-runtime-split-map.sh
python3 -m unittest discover -s .github/scripts -p "test_github_issues_runtime_split_map_contract.py"
```

## Public API Impact

- GitHub Issues runtime public entrypoints and bridge configuration surfaces
  remain stable.
- Webhook ingest and issue-comment processing payload contracts remain
  unchanged.
- Reason-code and error-envelope semantics stay behaviorally compatible.

## Import Impact

- Domain modules are extracted into
  `crates/tau-github-issues-runtime/src/github_issues_runtime/`.
- `github_issues_runtime.rs` keeps targeted re-exports during phased moves.
- Each phase minimizes cross-domain import churn and preserves bridge call-site
  stability.

## Test Migration Plan

- Guardrail update: enforce `github_issues_runtime.rs` split threshold ending
  at `<3000`.
- Crate-level validation: run `cargo test -p tau-github-issues-runtime` after
  each extraction phase.
- Cross-crate regression: run `cargo test -p tau-coding-agent` after each
  phase to confirm GitHub Issues bridge consumption parity.
