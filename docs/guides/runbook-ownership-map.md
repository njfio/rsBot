# Runbook Ownership Map

This guide defines post-consolidation ownership for operator-facing runbooks.

Use this map when triaging drift between documentation and runtime behavior.

| Runbook | Primary ownership surfaces | Notes |
| --- | --- | --- |
| `docs/guides/demo-index.md` | `crates/tau-coding-agent`, `scripts/demo/`, `crates/tau-gateway`, `crates/tau-multi-channel`, `crates/tau-deployment` | Demo wrappers prove retained runtime capabilities after consolidation waves. |
| `docs/guides/training-ops.md` | `crates/tau-trainer`, `crates/tau-training-runner`, `crates/tau-training-store`, `crates/tau-algorithm`, `crates/tau-coding-agent` | Prompt optimization control plane and store ownership boundaries. |
| `docs/guides/training-proxy-ops.md` | `crates/tau-training-proxy`, `crates/tau-gateway`, `crates/tau-provider`, `crates/tau-coding-agent` | Proxy attribution + upstream routing ownership boundaries. |
| `docs/guides/training-crate-boundary-plan.md` | `scripts/dev/training-crate-boundary-plan.sh`, `crates/tau-trainer`, `crates/tau-algorithm`, `crates/tau-training-store` | Canonical merge/retain decision plan and staged consolidation PR sets. |
| `docs/guides/transports.md` | `crates/tau-coding-agent`, `crates/tau-github-issues-runtime`, `crates/tau-slack-runtime`, `crates/tau-multi-channel`, `crates/tau-gateway`, `crates/tau-memory` | Transport entrypoints and per-surface runtime ownership map. |
| `docs/guides/memory-ops.md` | `crates/tau-agent-core`, `crates/tau-memory`, `crates/tau-tools`, `crates/tau-coding-agent` | Runtime memory behavior is owned by `tau-agent-core`; `tau-memory` owns shared storage helpers/contracts. |
| `docs/guides/dashboard-ops.md` | `crates/tau-dashboard`, `crates/tau-gateway`, `crates/tau-coding-agent` | Dashboard diagnostics, API/SSE surfaces, and CLI control-plane ownership boundaries. |
| `docs/guides/custom-command-ops.md` | `crates/tau-custom-command`, `crates/tau-coding-agent`, `crates/tau-tools` | Custom-command diagnostics and preserved state ownership boundaries after contract-runner removal. |
| `docs/guides/consolidated-runtime-rollback-drill.md` | `scripts/demo/rollback-drill-checklist.sh`, `scripts/dev/m21-retained-capability-proof-summary.sh`, `docs/guides/runbook-ownership-map.md` | Rollback trigger contract + artifact capture drill for consolidated runtime surfaces. |

## Ownership update policy

When crate boundaries change:

1. Update this table first.
2. Update the affected runbook `## Ownership` sections.
3. Run `.github/scripts/runbook_ownership_docs_check.py`.
4. Include ownership updates in the linked issue/PR acceptance evidence.
