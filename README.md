# rust-pi

Pure Rust implementation of core `pi-mono` concepts.

This workspace mirrors the high-level package boundaries from `badlogic/pi-mono` and provides a functional baseline:

- `crates/pi-ai`: provider-agnostic message and tool model + OpenAI/Anthropic/Google adapters
- `crates/pi-agent-core`: event-driven agent loop with tool execution
- `crates/pi-tui`: minimal differential terminal rendering primitives
- `crates/pi-coding-agent`: CLI harness with built-in `read`, `write`, `edit`, and `bash` tools

## Current Scope

Implemented now:

- Rust-first core architecture (no Node/TypeScript runtime)
- Tool-call loop (`assistant -> tool -> assistant`) in `pi-agent-core`
- Multi-provider model routing: `openai/*`, `anthropic/*`, `google/*`
- Interactive CLI and one-shot prompt mode
- Token-by-token CLI output rendering controls
- Persistent JSONL sessions with branch/resume support
- Session repair and lineage compaction commands
- Built-in filesystem and shell tools
- Theme loading and ANSI styling primitives in `pi-tui`
- Overlay composition primitives in `pi-tui`
- Editor buffer primitives in `pi-tui` (cursor + insert/delete/navigation)
- Skill loading from markdown packages via `--skills-dir` and `--skill`
- Remote skill fetch/install with optional checksum verification
- Registry-based skill installation (`--skill-registry-url`, `--install-skill-from-registry`)
- Signed registry skill installation with trust roots (`--skill-trust-root`, `--require-signed-skills`)
- Unit tests for serialization, tool loop, renderer diffing, and tool behaviors

Not implemented yet:

- Full TUI parity with overlays/images/editor

## Build & Test

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Usage

Set an API key for your provider:

```bash
# OpenAI-compatible
export OPENAI_API_KEY=...your-key...

# Anthropic
export ANTHROPIC_API_KEY=...your-key...

# Google Gemini
export GEMINI_API_KEY=...your-key...
```

Run interactive mode:

```bash
cargo run -p pi-coding-agent -- --model openai/gpt-4o-mini
```

Use Anthropic:

```bash
cargo run -p pi-coding-agent -- --model anthropic/claude-sonnet-4-20250514
```

Use Google Gemini:

```bash
cargo run -p pi-coding-agent -- --model google/gemini-2.5-pro
```

Run one prompt:

```bash
cargo run -p pi-coding-agent -- --prompt "Summarize src/lib.rs"
```

Cancel an in-flight prompt (interactive or one-shot) with `Ctrl+C`. The pending turn is discarded and session history remains consistent.

Control output streaming behavior:

```bash
# Disable token-by-token rendering
cargo run -p pi-coding-agent -- --prompt "Hello" --stream-output false

# Add artificial delay between streamed chunks
cargo run -p pi-coding-agent -- --prompt "Hello" --stream-delay-ms 20
```

Load reusable skills into the system prompt:

```bash
cargo run -p pi-coding-agent -- \
  --prompt "Review src/lib.rs" \
  --skills-dir .pi/skills \
  --skill checklist,security
```

Install skills into the local package directory before running:

```bash
cargo run -p pi-coding-agent -- \
  --prompt "Audit this module" \
  --skills-dir .pi/skills \
  --install-skill /tmp/review.md \
  --skill review
```

Install a remote skill with checksum verification:

```bash
cargo run -p pi-coding-agent -- \
  --prompt "Audit this module" \
  --skills-dir .pi/skills \
  --install-skill-url https://example.com/skills/review.md \
  --install-skill-sha256 2f7d0... \
  --skill review
```

Install skills from a remote registry manifest:

```bash
cargo run -p pi-coding-agent -- \
  --prompt "Audit this module" \
  --skills-dir .pi/skills \
  --skill-registry-url https://example.com/registry.json \
  --skill-registry-sha256 3ac10... \
  --install-skill-from-registry review \
  --skill review
```

Enforce signed registry skills with trusted root keys:

```bash
cargo run -p pi-coding-agent -- \
  --prompt "Audit this module" \
  --skills-dir .pi/skills \
  --skill-registry-url https://example.com/registry.json \
  --install-skill-from-registry review \
  --skill-trust-root root=Gf7... \
  --require-signed-skills \
  --skill review
```

Use a custom base URL (OpenAI-compatible):

```bash
cargo run -p pi-coding-agent -- --api-base http://localhost:11434/v1 --model openai/qwen2.5-coder
```

Session branching and resume:

```bash
# Persist to the default session file (.pi/sessions/default.jsonl)
cargo run -p pi-coding-agent -- --model openai/gpt-4o-mini

# Resume latest branch (default behavior), inspect session state
/session
/branches

# Switch to an older entry and fork a new branch
/branch 12

# Jump back to latest head
/resume

# Repair malformed/corrupted session graphs
/session-repair

# Compact to the active lineage and prune inactive branches
/session-compact
```

Tool policy controls:

```bash
cargo run -p pi-coding-agent -- \
  --model openai/gpt-4o-mini \
  --allow-path /Users/me/project \
  --max-file-read-bytes 500000 \
  --max-tool-output-bytes 8000 \
  --bash-timeout-ms 60000 \
  --max-command-length 2048
```
