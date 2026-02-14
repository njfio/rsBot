# Documentation Index

This index maps Tau documentation by audience and task.

| Audience | Start Here | Scope |
| --- | --- | --- |
| New user / operator | [Quickstart Guide](guides/quickstart.md) | Onboarding, auth modes, first prompt, first TUI run |
| Fresh-clone validator / demo operator | [Demo Index Guide](guides/demo-index.md) | Deterministic onboarding, gateway auth, multi-channel live ingest, and deployment WASM demos |
| Release validator / rollout operator | [Unified Live-Run Harness Guide](guides/live-run-unified-ops.md) | Single command cross-surface live validation manifest for voice/browser/dashboard/custom-command/memory |
| Release manager / canary operator | [Release Sign-Off Checklist](guides/release-signoff-checklist.md) | Mandatory evidence checklist covering preflight, 5/25/50 canaries, rollback readiness, and 100% promotion sign-off |
| Workspace operator | [Project Index Guide](guides/project-index.md) | Build/query/inspect deterministic local code index |
| Runtime operator / SRE | [Operator Control Summary](guides/operator-control-summary.md) | Unified control-plane status, policy posture, daemon/release checks, triage map |
| Gateway auth operator | [Gateway Auth Session Smoke](guides/gateway-auth-session-smoke.md) | End-to-end password-session issuance, authorized status call, invalid/expired fail-closed checks |
| Platform / integration engineer | [Transport Guide](guides/transports.md) | GitHub Issues bridge, Slack bridge, contract runners (multi-channel/multi-agent/memory/dashboard/gateway/deployment/custom-command/voice), RPC, ChannelStore admin |
| Package and extension author | [Packages Guide](guides/packages.md) | Extension manifests, package lifecycle, activation, signing |
| Scheduler / automation operator | [Events Guide](guides/events.md) | Events inspect/validate/simulate, runner, webhook ingest |
| Contributor to `tau-coding-agent` internals | [Code Map](tau-coding-agent/code-map.md) | Module ownership and architecture navigation |
| Contributor to `tau-coding-agent` refactor | [Crate Boundary Plan](tau-coding-agent/crate-boundary-plan.md) | Decomposition goals, crate layout, and migration phases |
| Contributor to runtime concurrency features | [Concurrent Agent Model](tau-coding-agent/concurrent-agent-model.md) | Forking model, parallel prompt API, isolation boundaries, and migration guidance |
| Provider auth implementer / reviewer | [Provider Auth Capability Matrix](provider-auth/provider-auth-capability-matrix.md) | Provider-mode support and implementation gates |

## Companion references

- Project overview: [`README.md`](../README.md)
- Examples and starter assets: [`examples/README.md`](../examples/README.md)
