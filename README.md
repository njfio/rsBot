# Tau

Tau is a pure-Rust agent runtime and operator control plane.

The workspace includes core agent execution, multi-provider model access, transport bridges,
gateway APIs, deterministic demo/contract workflows, and prompt-optimization training support.

Training boundary status:
- Canonical CLI training mode today is prompt optimization (`--prompt-optimization-*`).
- True-RL building blocks are also present in-repo (PPO/GAE components in `tau-algorithm`,
  collector/reward shaping runtime in `tau-training-runner`, and M24 proof tooling under
  `scripts/demo/m24-rl-*`).
- Historical true-RL delivery trackers are closed:
  [Epic #1657](https://github.com/njfio/Tau/issues/1657) and
  [Milestone #24](https://github.com/njfio/Tau/milestone/24)
  (`True RL Wave 2026-Q3: Policy Learning in Production`).
- Staged architecture/phase reference is documented in
  [`docs/planning/true-rl-roadmap-skeleton.md`](docs/planning/true-rl-roadmap-skeleton.md).

## What Tau Includes Today

- Rust-first runtime architecture (no Node.js/TypeScript runtime dependency in core paths)
- Provider-agnostic model routing (`openai/*`, `anthropic/*`, `google/*`) with multiple auth modes
- Interactive prompt loop, one-shot prompt mode, and plan-first orchestration mode
- Session persistence and lifecycle operations (branch, undo/redo, resume, export/import/repair)
- Built-in tools and tool-policy controls (filesystem/shell/http/path/rate/sandbox controls)
- GitHub Issues bridge and Slack Socket Mode bridge
- Multi-channel runtime (Telegram/Discord/WhatsApp) with live-ingress and connector paths
- Gateway OpenResponses/OpenAI-compatible HTTP APIs + websocket control plane + webchat shell
- Prompt optimization mode with SQLite-backed rollout state and optional attribution proxy
- True-RL algorithm and benchmark primitives (PPO/GAE + benchmark/safety proof scripts)
- Deterministic local demo scripts and CI smoke lanes

## Capability Status (Important)

Some subsystems are fully runnable in production-like loops, while others are currently
diagnostics-first or fixture/live-input driven:

- Voice:
  - `--voice-contract-runner` and `--voice-live-runner` are available.
  - Live mode consumes normalized input fixtures/files and writes deterministic artifacts.
  - Built-in microphone capture and end-user speech UX are not bundled in this repo.
- Browser automation:
  - `--browser-automation-live-runner` is available.
  - Execution is delegated to an external Playwright-compatible CLI (`--browser-automation-playwright-cli`).
  - No embedded browser engine/DOM automation runtime is built into Tau itself.
- Dashboard:
  - Gateway dashboard APIs and status/health inspection are available.
  - `--dashboard-contract-runner` is removed.
  - UI is a lightweight webchat shell plus API surfaces, not a full standalone rich dashboard app.
- Custom commands:
  - Status/health inspection and state artifacts are available.
  - `--custom-command-contract-runner` is removed from active dispatch.
  - Runtime crate includes command policy + execution primitives used by control-plane flows.
- Memory:
  - Runtime memory behavior is owned by `tau-agent-core`.
  - `tau-memory` provides memory store/retrieval primitives and contract fixtures/helpers.
  - `--memory-contract-runner` is removed.

For operational details and rollout/rollback guidance, use the runbooks in `docs/guides/`.

## Workspace Layout

High-level crates:

- `crates/tau-coding-agent`: main CLI runtime (`cargo run -p tau-coding-agent -- ...`)
- `crates/tau-agent-core`: event-driven agent loop and runtime memory integration
- `crates/tau-ai` + `crates/tau-provider`: model abstraction and provider auth/routing
- `crates/tau-gateway`: HTTP/websocket gateway surfaces
- `crates/tau-multi-channel`: Telegram/Discord/WhatsApp channel runtime
- `crates/tau-github-issues-runtime` + `crates/tau-slack-runtime`: bridge runtimes
- `crates/tau-training-*`, `crates/tau-algorithm`, `crates/tau-trainer`: prompt optimization pipeline
- `crates/tau-tui`: standalone terminal UI demo

Full workspace membership is defined in [`Cargo.toml`](Cargo.toml).

## Quickstart

Run all commands from repository root.

Build and validate fast (changed-scope default):

```bash
./scripts/dev/fast-validate.sh
```

Faster local compile-only loop:

```bash
./scripts/dev/fast-validate.sh --check-only --direct-packages-only --skip-fmt
```

Full pre-merge gate:

```bash
./scripts/dev/fast-validate.sh --full
```

Initialize workspace state:

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

## Container Packaging

Build and smoke-test the first-party Docker image locally:

```bash
./scripts/dev/docker-image-smoke.sh --tag tau-coding-agent:local-smoke
```

Run the image directly:

```bash
docker run --rm --entrypoint tau-coding-agent tau-coding-agent:local-smoke --help
```

## Homebrew Packaging

Each tagged release publishes a deterministic Homebrew formula asset (`tau.rb`)
generated from release `SHA256SUMS`.

```bash
# Install a pinned release formula directly from GitHub Releases
brew install --formula https://github.com/<owner>/Tau/releases/download/<release-tag>/tau.rb

# Upgrade installed formula (when newer formula/release exists)
brew upgrade tau

# Remove Tau from Homebrew
brew uninstall tau
```

## Shell Completions

Each tagged release also publishes shell completion assets:

- `tau-coding-agent.bash`
- `tau-coding-agent.zsh`
- `tau-coding-agent.fish`

```bash
# Bash
curl -fsSL -o ~/.local/share/bash-completion/completions/tau-coding-agent \
  https://github.com/<owner>/Tau/releases/download/<release-tag>/tau-coding-agent.bash

# Zsh
curl -fsSL -o ~/.zsh/completions/_tau-coding-agent \
  https://github.com/<owner>/Tau/releases/download/<release-tag>/tau-coding-agent.zsh

# Fish
curl -fsSL -o ~/.config/fish/completions/tau-coding-agent.fish \
  https://github.com/<owner>/Tau/releases/download/<release-tag>/tau-coding-agent.fish
```

## Demo Commands

Fresh-clone validation index:

```bash
./scripts/demo/index.sh
./scripts/demo/index.sh --list
./scripts/demo/index.sh --only onboarding,gateway-auth,gateway-remote-access --fail-fast
./scripts/demo/index.sh --json --report-file .tau/reports/demo-index-summary.json
```

Run all wrappers (single build reuse, optional filtering):

```bash
./scripts/demo/all.sh
./scripts/demo/all.sh --list
./scripts/demo/all.sh --only local,rpc,events --fail-fast
./scripts/demo/all.sh --only browser-automation,browser-automation-live --fail-fast
./scripts/demo/all.sh --only deployment,voice --fail-fast
./scripts/demo/all.sh --json --report-file .tau/reports/demo-summary.json
```

Individual wrappers:

- `./scripts/demo/local.sh`
- `./scripts/demo/rpc.sh`
- `./scripts/demo/events.sh`
- `./scripts/demo/package.sh`
- `./scripts/demo/multi-channel.sh`
- `./scripts/demo/multi-agent.sh`
- `./scripts/demo/browser-automation.sh`
- `./scripts/demo/browser-automation-live.sh`
- `./scripts/demo/memory.sh`
- `./scripts/demo/dashboard.sh`
- `./scripts/demo/gateway.sh`
- `./scripts/demo/gateway-auth.sh`
- `./scripts/demo/gateway-auth-session.sh`
- `./scripts/demo/gateway-remote-access.sh`
- `./scripts/demo/deployment.sh`
- `./scripts/demo/custom-command.sh`
- `./scripts/demo/voice.sh`
- `./scripts/demo/voice-live.sh`

`all.sh --json` and `--report-file` include `duration_ms` per wrapper.

Clean generated local artifacts:

```bash
./scripts/dev/clean-local-artifacts.sh
```

## Documentation

Start here: [`docs/README.md`](docs/README.md)

Core guides:

- Quickstart: [`docs/guides/quickstart.md`](docs/guides/quickstart.md)
- Demo index: [`docs/guides/demo-index.md`](docs/guides/demo-index.md)
- Transport surfaces: [`docs/guides/transports.md`](docs/guides/transports.md)
- Operator control summary: [`docs/guides/operator-control-summary.md`](docs/guides/operator-control-summary.md)
- Project index workflow: [`docs/guides/project-index.md`](docs/guides/project-index.md)
- Startup DI pipeline: [`docs/guides/startup-di-pipeline.md`](docs/guides/startup-di-pipeline.md)
- Contract lifecycle: [`docs/guides/contract-pattern-lifecycle.md`](docs/guides/contract-pattern-lifecycle.md)
- Multi-channel event pipeline: [`docs/guides/multi-channel-event-pipeline.md`](docs/guides/multi-channel-event-pipeline.md)
- Prompt optimization ops: [`docs/guides/training-ops.md`](docs/guides/training-ops.md)
- Prompt optimization proxy ops: [`docs/guides/training-proxy-ops.md`](docs/guides/training-proxy-ops.md)
- True-RL staged roadmap reference: [`docs/planning/true-rl-roadmap-skeleton.md`](docs/planning/true-rl-roadmap-skeleton.md)
- Memory ops: [`docs/guides/memory-ops.md`](docs/guides/memory-ops.md)
- Dashboard ops: [`docs/guides/dashboard-ops.md`](docs/guides/dashboard-ops.md)
- Custom command ops: [`docs/guides/custom-command-ops.md`](docs/guides/custom-command-ops.md)
- Voice ops: [`docs/guides/voice-ops.md`](docs/guides/voice-ops.md)
- Deployment ops: [`docs/guides/deployment-ops.md`](docs/guides/deployment-ops.md)
- Doc density scorecard: [`docs/guides/doc-density-scorecard.md`](docs/guides/doc-density-scorecard.md)

Contributor references:

- `tau-coding-agent` code map: [`docs/tau-coding-agent/code-map.md`](docs/tau-coding-agent/code-map.md)
- Provider auth matrix: [`docs/provider-auth/provider-auth-capability-matrix.md`](docs/provider-auth/provider-auth-capability-matrix.md)

## CI/CD

- CI: [`.github/workflows/ci.yml`](.github/workflows/ci.yml)
  - Linux quality gate (`fmt`, strict `clippy`, tests)
  - targeted `codex-light` and scoped smoke lanes
  - optional manual coverage and cross-platform compile smoke
- Security: [`.github/workflows/security.yml`](.github/workflows/security.yml)
- Release: [`.github/workflows/release.yml`](.github/workflows/release.yml)
  - Linux/macOS/Windows artifacts (`amd64`, `arm64`)
  - GHCR Docker image publish (`ghcr.io/<owner>/tau-coding-agent:<release-tag>`, `latest`)
  - Homebrew formula asset publish (`tau.rb`) derived from `SHA256SUMS`
  - shell completion assets (`tau-coding-agent.bash`, `.zsh`, `.fish`)
  - optional signing/notarization hooks (`scripts/release/hooks/*`)
  - installer/update scripts for Unix and PowerShell
- Dependabot: [`.github/dependabot.yml`](.github/dependabot.yml)

## Contributing Workflow

This repository follows issue-first execution:

- Create or identify a GitHub issue before implementation.
- Use scoped branches (`codex/issue-<id>-<topic>`).
- Link issues in PRs (`Closes #<id>`) and include validation evidence.
- Merge only after CI passes and acceptance criteria are met.

Repository policy details are in [`AGENTS.md`](AGENTS.md).
