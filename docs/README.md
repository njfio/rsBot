# Documentation Index

This index maps Tau documentation by audience and task.

| Audience | Start Here | Scope |
| --- | --- | --- |
| New user / operator | [Quickstart Guide](guides/quickstart.md) | Onboarding, auth modes, first prompt, first TUI run |
| Fresh-clone validator / demo operator | [Demo Index Guide](guides/demo-index.md) | Deterministic onboarding, gateway auth, multi-channel live ingest, and deployment WASM demos |
| Prompt optimization operator | [Prompt Optimization Operations Guide](guides/training-ops.md) | Rollout prompt optimization mode, JSON config schema, and SQLite-backed state |
| Prompt optimization integration operator | [Prompt Optimization Proxy Operations Guide](guides/training-proxy-ops.md) | OpenAI-compatible proxy mode with rollout/attempt attribution logs |
| Workspace operator | [Project Index Guide](guides/project-index.md) | Build/query/inspect deterministic local code index |
| Runtime operator / SRE | [Operator Control Summary](guides/operator-control-summary.md) | Unified control-plane status, policy posture, daemon/release checks, triage map |
| Runtime contributor | [Startup DI Pipeline](guides/startup-di-pipeline.md) | 3-stage startup resolution: preflight gate, dependency/context composition, mode dispatch |
| Runtime contributor | [Contract Pattern Lifecycle](guides/contract-pattern-lifecycle.md) | Shared fixture lifecycle, compatibility gates, extension checklist, anti-patterns |
| Runtime contributor | [Tool Name Registry](guides/tool-name-registry.md) | Reserved built-in tool-name catalog and registration conflict behavior for extension + MCP external tools |
| Runtime contributor | [Tool Policy Sandbox Mode](guides/tool-policy-sandbox-mode.md) | Fail-closed sandbox posture (`best-effort` vs `required`), preset defaults, and operator diagnostics |
| Runtime contributor | [Tool Policy HTTP Client](guides/tool-policy-http-client.md) | `HttpTool` controls, SSRF/redirect guardrails, caps, and deterministic reason codes |
| Runtime contributor | [Tool Policy Protected Paths](guides/tool-policy-protected-paths.md) | Protected identity/system file deny policy for write/edit with override controls and diagnostics |
| Multi-channel contributor | [Multi-channel Event Pipeline](guides/multi-channel-event-pipeline.md) | Inbound normalization, policy/pairing, routing, persistence, outbound retry paths |
| Runtime maintainer | [Doc Density Scorecard](guides/doc-density-scorecard.md) | Baseline/targets for public API docs coverage and CI regression guard policy |
| Roadmap operator | [Roadmap Execution Index](guides/roadmap-execution-index.md) | End-to-end mapping from `tasks/todo.md` items to milestones/issues and execution wave ordering |
| Gateway auth operator | [Gateway Auth Session Smoke](guides/gateway-auth-session-smoke.md) | End-to-end password-session issuance, authorized status call, invalid/expired fail-closed checks |
| Platform / integration engineer | [Transport Guide](guides/transports.md) | GitHub Issues bridge, Slack bridge, contract runners (multi-channel/multi-agent/gateway/deployment/voice), dashboard diagnostics/API, memory diagnostics, custom-command diagnostics, RPC, ChannelStore admin |
| Package and extension author | [Packages Guide](guides/packages.md) | Extension manifests, package lifecycle, activation, signing |
| Scheduler / automation operator | [Events Guide](guides/events.md) | Events inspect/validate/simulate, runner, webhook ingest |
| Contributor to `tau-coding-agent` internals | [Code Map](tau-coding-agent/code-map.md) | Module ownership and architecture navigation |
| Contributor to `tau-coding-agent` refactor | [Crate Boundary Plan](tau-coding-agent/crate-boundary-plan.md) | Decomposition goals, crate layout, and migration phases |
| Contributor to runtime concurrency features | [Concurrent Agent Model](tau-coding-agent/concurrent-agent-model.md) | Forking model, parallel prompt API, isolation boundaries, and migration guidance |
| Provider auth implementer / reviewer | [Provider Auth Capability Matrix](provider-auth/provider-auth-capability-matrix.md) | Provider-mode support and implementation gates |

## Companion references

- Project overview: [`README.md`](../README.md)
- Examples and starter assets: [`examples/README.md`](../examples/README.md)
