# PR Batch Lane Boundaries

This guide defines lane-specific file ownership boundaries for parallel PR batching.

Machine-readable source of truth: `tasks/policies/pr-batch-lane-boundaries.json`

## Lane Boundary Map

Use one lane per PR unless there is an explicit cross-lane exception in issue comments.

| Lane | Primary ownership boundaries | Shared paths (hotspot rules apply) |
| --- | --- | --- |
| `structural` | `crates/tau-cli/`, `crates/tau-coding-agent/`, `crates/tau-tools/`, `crates/tau-channel-store/`, `crates/tau-github-issues-runtime/`, `crates/tau-slack-runtime/`, `crates/tau-safety/` | `Cargo.toml`, `Cargo.lock`, `.github/workflows/ci.yml` |
| `docs` | `docs/`, `tasks/todo.md`, `tasks/tau-vs-ironclaw-gap-list.md`, `tasks/reports/`, docs link-check scripts/workflow | `README.md`, `docs/README.md`, `docs/guides/runbook-ownership-map.md` |
| `rl` | `crates/tau-algorithm/`, `crates/tau-trainer/`, `crates/tau-training-*`, training runbooks | `crates/tau-cli/src/cli_args.rs`, `Cargo.toml`, `Cargo.lock`, `.github/workflows/ci.yml` |

## Conflict Hotspots And Mitigation Notes

| Hotspot ID | Path | Why it conflicts | Mitigation |
| --- | --- | --- | --- |
| `hotspot.workspace-manifests` | `Cargo.toml` | Cross-lane dependency edits | Rebase before merge; queue one manifest-changing PR at a time; include dependency delta summary. |
| `hotspot.workspace-lockfile` | `Cargo.lock` | Lockfile churn from concurrent dependency updates | Regenerate only after rebase; avoid lockfile-only drift commits; rerun `cargo check` after merge conflict resolution. |
| `hotspot.quality-workflow` | `.github/workflows/ci.yml` | Multiple lanes patch same workflow jobs | Keep lane-local scope and ordering; update contract tests in same PR; keep path filters narrow. |
| `hotspot.roadmap-doc-blocks` | `tasks/todo.md` | Generated status blocks reflow on sync | Run `scripts/dev/roadmap-status-sync.sh` before final push; avoid manual edits in generated blocks. |
| `hotspot.runbook-ownership-map` | `docs/guides/runbook-ownership-map.md` | Structural and RL runbook updates collide | Prefer docs-lane ownership-map batching; run `.github/scripts/runbook_ownership_docs_check.py`. |

## Active PR Reference Contract

All active PRs must reference the boundary map and lane in the PR description using `.github/pull_request_template.md`.

Required fields:

- `Lane`
- `Boundary Map`
- `Boundary Paths`
- `Hotspot Mitigation`

Example:

```text
Lane: structural
Boundary Map: tasks/policies/pr-batch-lane-boundaries.json
Boundary Paths: crates/tau-tools/src/tools.rs; crates/tau-tools/src/registry.rs
Hotspot Mitigation: none (no hotspot files changed)
```
