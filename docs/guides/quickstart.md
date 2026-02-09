# Quickstart Guide

Run all commands from repository root.

## Build and test

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Onboarding

Initialize `.tau` directories, profile store, and release-channel metadata:

```bash
cargo run -p tau-coding-agent -- --onboard --onboard-non-interactive
```

Interactive onboarding mode:

```bash
cargo run -p tau-coding-agent -- --onboard --onboard-profile default
```

## Auth modes

| Provider | Local/dev recommended | CI/automation recommended |
| --- | --- | --- |
| OpenAI | `--openai-auth-mode oauth-token` or `session-token` with Codex backend (`--openai-codex-backend=true`) | `--openai-auth-mode api-key` with `OPENAI_API_KEY` |
| Anthropic | `--anthropic-auth-mode oauth-token` or `session-token` with Claude backend (`--anthropic-claude-backend=true`) | `--anthropic-auth-mode api-key` with `ANTHROPIC_API_KEY` |
| Google | `--google-auth-mode oauth-token` (Gemini login) or `--google-auth-mode adc` (Vertex/ADC) with Gemini backend (`--google-gemini-backend=true`) | `--google-auth-mode api-key` with `GEMINI_API_KEY` |

## First run

Interactive prompt loop:

```bash
cargo run -p tau-coding-agent -- --model openai/gpt-4o-mini
```

One-shot prompt:

```bash
cargo run -p tau-coding-agent -- --prompt "Summarize src/lib.rs"
```

Plan-first orchestration mode:

```bash
cargo run -p tau-coding-agent -- \
  --prompt "Summarize src/lib.rs" \
  --orchestrator-mode plan-first \
  --orchestrator-delegate-steps
```

## Provider login paths (subscription workflows)

OpenAI/Codex:

```bash
codex --login
cargo run -p tau-coding-agent -- --model openai/gpt-4o-mini --openai-auth-mode oauth-token
```

Anthropic/Claude Code:

```bash
claude
# run /login in claude, then:
cargo run -p tau-coding-agent -- --model anthropic/claude-sonnet-4-20250514 --anthropic-auth-mode oauth-token
```

Google/Gemini:

```bash
gemini
cargo run -p tau-coding-agent -- --model google/gemini-2.5-pro --google-auth-mode oauth-token
```

## Run the TUI demo

```bash
cargo run -p tau-tui -- --frames 2 --sleep-ms 0 --width 56 --no-color
```

## Run deterministic local demos

```bash
# all.sh prepares the binary once, then reuses it across selected wrappers.
./scripts/demo/all.sh
./scripts/demo/all.sh --list
./scripts/demo/all.sh --only rpc,events --json
./scripts/demo/all.sh --report-file .tau/reports/demo-summary.json
./scripts/demo/all.sh --only local,rpc --fail-fast
./scripts/demo/all.sh --only local --timeout-seconds 30 --fail-fast
./scripts/demo/local.sh
./scripts/demo/rpc.sh
./scripts/demo/events.sh
./scripts/demo/package.sh
```

`all.sh --json` and report-file payloads include `duration_ms` per wrapper entry.

All wrappers support `--skip-build` and `--binary <path>` for prebuilt binaries.

## Cleanup local generated artifacts

```bash
./scripts/dev/clean-local-artifacts.sh
```
