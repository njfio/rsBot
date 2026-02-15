# Tau

Pure Rust implementation of core upstream mono concepts.

This workspace mirrors the high-level package boundaries from the upstream mono baseline:

- `crates/tau-ai`: provider-agnostic message and tool model + OpenAI/Anthropic/Google adapters
- `crates/tau-agent-core`: event-driven agent loop with tool execution
- `crates/tau-tui`: terminal rendering primitives plus runnable demo binary
- `crates/tau-coding-agent`: CLI harness with built-in tools and transport bridges

## Documentation

Start with the docs index: [`docs/README.md`](docs/README.md)

Focused guides:

- Quickstart: [`docs/guides/quickstart.md`](docs/guides/quickstart.md)
- Demo index: [`docs/guides/demo-index.md`](docs/guides/demo-index.md)
- Gateway auth session smoke: [`docs/guides/gateway-auth-session-smoke.md`](docs/guides/gateway-auth-session-smoke.md)
- Project index workflow: [`docs/guides/project-index.md`](docs/guides/project-index.md)
- Transports (GitHub/Slack/RPC): [`docs/guides/transports.md`](docs/guides/transports.md)
- Startup DI pipeline: [`docs/guides/startup-di-pipeline.md`](docs/guides/startup-di-pipeline.md)
- Contract pattern lifecycle: [`docs/guides/contract-pattern-lifecycle.md`](docs/guides/contract-pattern-lifecycle.md)
- Multi-channel event pipeline: [`docs/guides/multi-channel-event-pipeline.md`](docs/guides/multi-channel-event-pipeline.md)
- Operator control summary: [`docs/guides/operator-control-summary.md`](docs/guides/operator-control-summary.md)
- Doc density scorecard: [`docs/guides/doc-density-scorecard.md`](docs/guides/doc-density-scorecard.md)
- Packages and extensions: [`docs/guides/packages.md`](docs/guides/packages.md)
- Events and scheduler: [`docs/guides/events.md`](docs/guides/events.md)

Contributor references:

- `tau-coding-agent` architecture map: [`docs/tau-coding-agent/code-map.md`](docs/tau-coding-agent/code-map.md)
- Provider auth capability matrix: [`docs/provider-auth/provider-auth-capability-matrix.md`](docs/provider-auth/provider-auth-capability-matrix.md)

## Quickstart

Run all commands from repository root.

Build/test:

```bash
./scripts/dev/fast-validate.sh
# full pre-merge gate:
./scripts/dev/fast-validate.sh --full
```

Bootstrap Tau workspace:

```bash
cargo run -p tau-coding-agent -- --onboard --onboard-non-interactive
```

Run interactive mode:

```bash
cargo run -p tau-coding-agent -- --model openai/gpt-4o-mini
```

Run one-shot mode:

```bash
cargo run -p tau-coding-agent -- --prompt "Summarize src/lib.rs"
```

Run the standalone TUI demo:

```bash
cargo run -p tau-tui -- --frames 2 --sleep-ms 0 --width 56 --no-color
```

Run deterministic local demos:

```bash
# operator-focused fresh-clone validation path
./scripts/demo/index.sh
./scripts/demo/index.sh --list
./scripts/demo/index.sh --only onboarding,gateway-auth --fail-fast
./scripts/demo/index.sh --json --report-file .tau/reports/demo-index-summary.json

# all.sh prepares the binary once, then reuses it across selected wrappers.
./scripts/demo/all.sh
./scripts/demo/all.sh --list
./scripts/demo/all.sh --only rpc,events --json
./scripts/demo/all.sh --report-file .tau/reports/demo-summary.json
./scripts/demo/all.sh --only local,rpc --fail-fast
./scripts/demo/all.sh --only multi-agent --fail-fast
./scripts/demo/all.sh --only gateway --fail-fast
./scripts/demo/all.sh --only deployment --fail-fast
./scripts/demo/all.sh --only custom-command --fail-fast
./scripts/demo/all.sh --only voice --fail-fast
./scripts/demo/all.sh --only local --timeout-seconds 30 --fail-fast
./scripts/demo/local.sh
./scripts/demo/rpc.sh
./scripts/demo/events.sh
./scripts/demo/package.sh
./scripts/demo/multi-channel.sh
./scripts/demo/multi-agent.sh
./scripts/demo/memory.sh
./scripts/demo/dashboard.sh
./scripts/demo/gateway.sh
./scripts/demo/gateway-auth.sh
./scripts/demo/gateway-auth-session.sh
./scripts/demo/deployment.sh
./scripts/demo/custom-command.sh
./scripts/demo/voice.sh
```

`all.sh --json` and `--report-file` entries include `duration_ms` per wrapper.

Clean generated local artifacts/noise:

```bash
./scripts/dev/clean-local-artifacts.sh
```

Example assets referenced by guides and smoke tests:

- `./examples/starter/package.json`
- `./examples/extensions`
- `./examples/extensions/issue-assistant/extension.json`
- `./examples/extensions/issue-assistant/payload.json`
- `./examples/events`
- `./examples/events-state.json`

## Current Scope

Implemented now:

- Rust-first runtime architecture (no Node/TypeScript runtime)
- Multi-provider model routing (`openai/*`, `anthropic/*`, `google/*`)
- OAuth/session login backend routing for Codex, Claude Code, and Gemini CLI flows
- Interactive prompt mode, one-shot mode, and plan-first orchestration mode
- Persistent JSONL sessions with branch/undo/redo/resume/repair/export/import tooling
- Deterministic project index build/query/inspect workflow for local code search
- Built-in filesystem and shell tools
- Transport bridges for GitHub Issues and Slack Socket Mode
- Startup DI, contract lifecycle, and multi-channel event pipeline architecture guides
- RPC capabilities/dispatch/serve NDJSON protocol surfaces
- Filesystem-backed scheduled events engine with webhook ingest
- Extension and package lifecycle tooling (including signing and trust roots)
- Deterministic local demo scripts and smoke coverage
- Runnable `tau-tui` demo binary and smoke tests

## CI/CD

GitHub Actions workflows:

- CI: [`.github/workflows/ci.yml`](.github/workflows/ci.yml)
  - Linux quality gate (`fmt`, strict `clippy`, workspace tests)
  - bounded checkout retry diagnostics in `quality-linux`
  - codex-light demo-smoke lane
  - optional manual coverage and cross-platform compile smoke
- Security: [`.github/workflows/security.yml`](.github/workflows/security.yml)
- Release: [`.github/workflows/release.yml`](.github/workflows/release.yml)
  - Multi-platform artifacts: linux (`amd64`, `arm64`), macOS (`amd64`, `arm64`), windows (`amd64`)
  - Optional signing/notarization hook integration (`scripts/release/hooks/*`)
  - Installer helpers: `scripts/release/install-tau.sh`, `scripts/release/update-tau.sh`, `scripts/release/install-tau.ps1`, `scripts/release/update-tau.ps1`

Dependency automation: [`.github/dependabot.yml`](.github/dependabot.yml)
