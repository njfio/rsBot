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
- Session repair, export/import, and lineage compaction commands
- Built-in filesystem and shell tools
- Theme loading and ANSI styling primitives in `pi-tui`
- Overlay composition primitives in `pi-tui`
- Editor buffer primitives in `pi-tui` (cursor + insert/delete/navigation)
- Editor viewport rendering in `pi-tui` (line numbers + cursor marker)
- Image rendering primitives in `pi-tui` (grayscale-to-ASCII)
- Skill loading from markdown packages via `--skills-dir` and `--skill`
- Remote skill fetch/install with optional checksum verification
- Registry-based skill installation (`--skill-registry-url`, `--install-skill-from-registry`)
- Signed registry skill installation with trust roots (`--skill-trust-root`, `--require-signed-skills`)
- Skills lockfile write/sync workflow (`--skills-lock-write`, `--skills-sync`)
- Unit tests for serialization, tool loop, renderer diffing, and tool behaviors

## Build & Test

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Robustness and performance suites:

```bash
# Property and regression-heavy suite
cargo test -p pi-coding-agent

# Stress-oriented session test
cargo test -p pi-coding-agent stress_parallel_appends_high_volume_remain_consistent

# Benchmarks
cargo bench -p pi-tui
```

## CI/CD

This repository includes GitHub Actions workflows for:

- CI (`.github/workflows/ci.yml`): format, clippy, and workspace tests on Linux/macOS/Windows
- Security (`.github/workflows/security.yml`): `cargo audit` and `cargo deny`
- Releases (`.github/workflows/release.yml`): build and publish `pi-coding-agent` assets on `v*` tags

Dependency update automation is configured in `.github/dependabot.yml`.

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

Run one prompt from a file:

```bash
cargo run -p pi-coding-agent -- --prompt-file .pi/prompts/review.txt
```

Pipe a one-shot prompt from stdin:

```bash
printf "Summarize src/main.rs" | cargo run -p pi-coding-agent -- --prompt-file -
```

Execute a slash-command script and exit:

```bash
# Fail-fast on first malformed/failing line (default)
cargo run -p pi-coding-agent -- --no-session --command-file .pi/commands/checks.commands

# Continue past malformed/failing lines and report a final summary
cargo run -p pi-coding-agent -- --no-session \
  --command-file .pi/commands/checks.commands \
  --command-file-error-mode continue-on-error
```

Load the base system prompt from a file:

```bash
cargo run -p pi-coding-agent -- \
  --system-prompt-file .pi/prompts/system.txt \
  --prompt "Review src/main.rs"
```

Cancel an in-flight prompt (interactive or one-shot) with `Ctrl+C`. The pending turn is discarded and session history remains consistent.

Control output streaming behavior:

```bash
# Disable token-by-token rendering
cargo run -p pi-coding-agent -- --prompt "Hello" --stream-output false

# Add artificial delay between streamed chunks
cargo run -p pi-coding-agent -- --prompt "Hello" --stream-delay-ms 20
```

When using OpenAI-compatible, Anthropic, or Google models with `--stream-output true`, the client uses provider-side incremental streaming when available.

Control provider and turn timeouts:

```bash
# Request timeout for provider HTTP calls
cargo run -p pi-coding-agent -- --prompt "Hello" --request-timeout-ms 60000

# Abort a single prompt turn if it exceeds 20 seconds (0 disables)
cargo run -p pi-coding-agent -- --prompt "Hello" --turn-timeout-ms 20000
```

Control provider retry resilience behavior:

```bash
# Retry retryable provider errors up to 4 times
cargo run -p pi-coding-agent -- --prompt "Hello" --provider-max-retries 4

# Enforce a 1500ms cumulative backoff budget for retries (0 disables)
cargo run -p pi-coding-agent -- --prompt "Hello" --provider-retry-budget-ms 1500

# Disable jitter to use deterministic exponential backoff
cargo run -p pi-coding-agent -- --prompt "Hello" --provider-retry-jitter false

# Configure fallback models (attempted in order on retriable provider failures)
cargo run -p pi-coding-agent -- --prompt "Hello" \
  --model openai/gpt-4o-mini \
  --fallback-model openai/gpt-4o,anthropic/claude-sonnet-4-20250514
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

Write a deterministic skills lockfile after installs:

```bash
cargo run -p pi-coding-agent -- \
  --prompt "Audit this module" \
  --skills-dir .pi/skills \
  --install-skill /tmp/review.md \
  --skills-lock-write \
  --skill review
```

Verify installed skills match the lockfile (fails on drift):

```bash
cargo run -p pi-coding-agent -- \
  --skills-dir .pi/skills \
  --skills-sync \
  --no-session
```

Manage trust lifecycle in a trust-root file:

```bash
# Add/update a trust root key
cargo run -p pi-coding-agent -- \
  --prompt "noop" \
  --skill-trust-root-file .pi/skills/trust-roots.json \
  --skill-trust-add root=Gf7...

# Rotate a key (revokes old id and adds new id/key)
cargo run -p pi-coding-agent -- \
  --prompt "noop" \
  --skill-trust-root-file .pi/skills/trust-roots.json \
  --skill-trust-rotate root:root-v2=AbC...

# Revoke a key id
cargo run -p pi-coding-agent -- \
  --prompt "noop" \
  --skill-trust-root-file .pi/skills/trust-roots.json \
  --skill-trust-revoke root
```

Registry manifests can optionally mark keys/skills as revoked or expired with:

- `revoked: true`
- `expires_unix: <unix_timestamp_seconds>`

Use a custom base URL (OpenAI-compatible):

```bash
cargo run -p pi-coding-agent -- --api-base http://localhost:11434/v1 --model openai/qwen2.5-coder
```

Session branching and resume:

```bash
# Persist to the default session file (.pi/sessions/default.jsonl)
cargo run -p pi-coding-agent -- --model openai/gpt-4o-mini

# Show interactive command help
/help
/help branch

# Resume latest branch (default behavior), inspect session state
/session
/session-search retry budget
/session-stats
/doctor
/session-graph-export /tmp/session-graph.mmd
/branches

# Save/list/run repeatable command macros (project-local .pi/macros.json)
/macro save quick-check /tmp/quick-check.commands
/macro list
/macro run quick-check --dry-run
/macro run quick-check

# Save/load runtime defaults profiles (project-local .pi/profiles.json)
/profile save baseline
/profile list
/profile show baseline
/profile load baseline
/profile delete baseline

# Persist and use named aliases for fast branch navigation
/branch-alias set hotfix 12
/branch-alias list
/branch-alias use hotfix

# Switch to an older entry and fork a new branch
/branch 12

# Jump back to latest head
/resume

# Repair malformed/corrupted session graphs
/session-repair

# Compact to the active lineage and prune inactive branches
/session-compact

# Export the active lineage snapshot to a new JSONL file
/session-export /tmp/session-snapshot.jsonl

# Import a snapshot into the current session (mode defaults to merge)
/session-import /tmp/session-snapshot.jsonl

# Show a specific installed skill by name
/skills-show checklist

# Search installed skills by name/content with optional result cap
/skills-search checklist
/skills-search checklist 10

# Inspect lockfile drift without enforcing sync (optional JSON output)
/skills-lock-diff
/skills-lock-diff /tmp/custom-skills.lock.json --json

# Preview and apply prune of untracked local skills
/skills-prune
/skills-prune /tmp/custom-skills.lock.json --apply

# Inspect trust-root keys from configured or explicit trust-root file
/skills-trust-list
/skills-trust-list /tmp/trust-roots.json

# List currently installed skills
/skills-list

# Write/update skills lockfile from currently installed skills
/skills-lock-write
/skills-lock-write /tmp/custom-skills.lock.json

# Validate installed skills against lockfile drift without restarting
/skills-sync
/skills-sync /tmp/custom-skills.lock.json

# Repair/import output includes affected IDs and remap pairs for diagnostics
```

Tune session lock behavior for shared/concurrent workflows:

```bash
cargo run -p pi-coding-agent -- \
  --model openai/gpt-4o-mini \
  --session-lock-wait-ms 15000 \
  --session-lock-stale-ms 60000

# Optional: use replace mode for /session-import
cargo run -p pi-coding-agent -- \
  --model openai/gpt-4o-mini \
  --session-import-mode replace
```

Validate a session graph and exit:

```bash
cargo run -p pi-coding-agent -- \
  --session .pi/sessions/default.jsonl \
  --session-validate
```

Tool policy controls:

```bash
cargo run -p pi-coding-agent -- \
  --model openai/gpt-4o-mini \
  --tool-policy-preset hardened \
  --allow-path /Users/me/project \
  --max-file-read-bytes 500000 \
  --max-file-write-bytes 500000 \
  --max-tool-output-bytes 8000 \
  --bash-timeout-ms 60000 \
  --max-command-length 2048 \
  --bash-profile strict \
  --bash-dry-run false \
  --tool-policy-trace false \
  --allow-command python,cargo-nextest* \
  --print-tool-policy \
  --os-sandbox-mode auto \
  --os-sandbox-command bwrap,--die-with-parent,--new-session,--unshare-all,--proc,/proc,--dev,/dev,--tmpfs,/tmp,--bind,{cwd},{cwd},--chdir,{cwd},{shell},-lc,{command} \
  --enforce-regular-files true
```

Emit structured tool audit events to JSONL:

```bash
cargo run -p pi-coding-agent -- \
  --model openai/gpt-4o-mini \
  --prompt "Inspect repo status" \
  --tool-audit-log .pi/audit/tools.jsonl
```
