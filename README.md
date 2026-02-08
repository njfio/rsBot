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
- Remote/registry download cache with offline replay (`--skills-cache-dir`, `--skills-offline`)
- Skills lockfile write/sync workflow (`--skills-lock-write`, `--skills-sync`)
- Unit tests for serialization, tool loop, renderer diffing, and tool behaviors
- Provider auth/login feasibility matrix and implementation gates (`docs/provider-auth/provider-auth-capability-matrix.md`)

## Contributor Map

- `pi-coding-agent` architecture and module guide: `docs/pi-coding-agent/code-map.md`

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

- CI (`.github/workflows/ci.yml`): Linux quality gate (fmt + clippy + workspace tests) on PR/push, cross-platform compile smoke on push/manual, optional manual coverage run
  - Includes transport replay conformance gate (`transport_conformance_*`) for GitHub/Slack/scheduler fixtures when transport files change
- Security (`.github/workflows/security.yml`): consolidated `cargo audit` + `cargo deny` on default-branch pushes, weekly schedule, and manual runs
- Releases (`.github/workflows/release.yml`): build and publish `pi-coding-agent` assets on `v*` tags

Dependency update automation is configured in `.github/dependabot.yml`.

## Usage

Set an API key for your provider:

```bash
# OpenAI-compatible
export OPENAI_API_KEY=...your-key...

# OpenRouter (OpenAI-compatible alias path)
export OPENROUTER_API_KEY=...your-openrouter-key...

# Groq (OpenAI-compatible alias path)
export GROQ_API_KEY=...your-groq-key...

# xAI (OpenAI-compatible alias path)
export XAI_API_KEY=...your-xai-key...

# Mistral (OpenAI-compatible alias path)
export MISTRAL_API_KEY=...your-mistral-key...

# Azure OpenAI (OpenAI-compatible runtime with Azure auth/query mode)
export AZURE_OPENAI_API_KEY=...your-azure-openai-key...

# Anthropic
export ANTHROPIC_API_KEY=...your-key...

# Google Gemini
export GEMINI_API_KEY=...your-key...
```

Run interactive mode:

```bash
cargo run -p pi-coding-agent -- --model openai/gpt-4o-mini
```

Run interactive mode with plan-first orchestration on each user turn:

```bash
cargo run -p pi-coding-agent -- \
  --model openai/gpt-4o-mini \
  --orchestrator-mode plan-first \
  --orchestrator-delegate-steps \
  --orchestrator-max-plan-steps 8 \
  --orchestrator-max-delegated-steps 8 \
  --orchestrator-max-executor-response-chars 20000 \
  --orchestrator-max-delegated-step-response-chars 20000 \
  --orchestrator-max-delegated-total-response-chars 160000
```

Plan-first mode emits deterministic orchestration traces for `planner`, `executor`, `delegated-step` (when enabled), `review`, and `consolidation` phases, including response budget metadata and accept/reject consolidation decisions.

Use Anthropic:

```bash
cargo run -p pi-coding-agent -- --model anthropic/claude-sonnet-4-20250514
```

Use Google Gemini:

```bash
cargo run -p pi-coding-agent -- --model google/gemini-2.5-pro
```

Use OpenRouter via OpenAI-compatible endpoint:

```bash
cargo run -p pi-coding-agent -- \
  --model openrouter/openai/gpt-4o-mini \
  --api-base https://openrouter.ai/api/v1
```

Use Groq via OpenAI-compatible endpoint:

```bash
cargo run -p pi-coding-agent -- \
  --model groq/llama-3.3-70b \
  --api-base https://api.groq.com/openai/v1
```

Use xAI via OpenAI-compatible endpoint:

```bash
cargo run -p pi-coding-agent -- \
  --model xai/grok-4 \
  --api-base https://api.x.ai/v1
```

Use Mistral via OpenAI-compatible endpoint:

```bash
cargo run -p pi-coding-agent -- \
  --model mistral/mistral-large-latest \
  --api-base https://api.mistral.ai/v1
```

Use Azure OpenAI deployment endpoint:

```bash
cargo run -p pi-coding-agent -- \
  --model azure/gpt-4o-mini \
  --api-base https://YOUR-RESOURCE.openai.azure.com/openai/deployments/YOUR-DEPLOYMENT \
  --azure-openai-api-version 2024-10-21
```

Refresh model catalog metadata from a remote JSON URL (cached locally):

```bash
cargo run -p pi-coding-agent -- \
  --model openai/gpt-4o-mini \
  --model-catalog-url https://example.com/models.json \
  --model-catalog-cache .pi/models/catalog.json
```

Inspect model capabilities in interactive mode:

```text
/models-list gpt --provider openai --tools true --limit 20
/model-show openai/gpt-4o-mini
```

Run one prompt:

```bash
cargo run -p pi-coding-agent -- --prompt "Summarize src/lib.rs"
```

Run one prompt with plan-first orchestration:

```bash
cargo run -p pi-coding-agent -- \
  --prompt "Summarize src/lib.rs" \
  --orchestrator-mode plan-first \
  --orchestrator-delegate-steps \
  --orchestrator-max-plan-steps 8 \
  --orchestrator-max-delegated-steps 8 \
  --orchestrator-max-executor-response-chars 20000 \
  --orchestrator-max-delegated-step-response-chars 20000 \
  --orchestrator-max-delegated-total-response-chars 160000
```

Run one prompt from a file:

```bash
cargo run -p pi-coding-agent -- --prompt-file .pi/prompts/review.txt
```

Run one prompt from a template file with variables:

```bash
cargo run -p pi-coding-agent -- \
  --prompt-template-file .pi/prompts/review.template.txt \
  --prompt-template-var module=src/main.rs \
  --prompt-template-var focus=\"error handling\"
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

Run as a GitHub Issues conversational transport ("living issue chat stream"):

```bash
export GITHUB_TOKEN=...your-token...

cargo run -p pi-coding-agent -- \
  --model openai/gpt-4o-mini \
  --github-issues-bridge \
  --github-repo owner/repo \
  --github-bot-login your-bot-login \
  --github-state-dir .pi/github-issues \
  --github-poll-interval-seconds 30 \
  --github-artifact-retention-days 30
```

In bridge mode:

- each issue maps to a deterministic session file under `.pi/github-issues/.../sessions/`
- inbound/outbound event payloads are persisted as JSONL logs for replay/debugging
- duplicate deliveries are deduplicated using persisted event keys and response footers
- bot replies include run/model/token metadata in the issue comment footer
- each completed run emits a markdown artifact plus metadata index entry under `channel-store/.../artifacts/`
- set `--github-artifact-retention-days 0` to disable artifact expiration
- `/pi artifacts` posts the current issue artifact inventory; `/pi artifacts run <run_id>` filters inventory for one run; `/pi artifacts show <artifact_id>` shows one artifact record; `/pi artifacts purge` removes expired entries and reports lifecycle counts

Run as a Slack Socket Mode conversational transport:

```bash
export PI_SLACK_APP_TOKEN=...xapp-token...
export PI_SLACK_BOT_TOKEN=...xoxb-token...

cargo run -p pi-coding-agent -- \
  --model openai/gpt-4o-mini \
  --slack-bridge \
  --slack-state-dir .pi/slack \
  --slack-artifact-retention-days 30 \
  --slack-thread-detail-output true \
  --slack-thread-detail-threshold-chars 1500
```

In Slack bridge mode:

- `app_mention` and DM (`message.im`) events are normalized into per-channel runs
- each channel maps to a deterministic session file under `.pi/slack/channels/.../session.jsonl`
- inbound/outbound event payloads are persisted as JSONL logs for replay/debugging
- duplicate deliveries are deduplicated via persisted event keys
- stale events are skipped based on `--slack-max-event-age-seconds`
- attached files are downloaded into channel-local attachment folders and surfaced in prompt context
- each completed run emits a markdown artifact + metadata under `channel-store/channels/slack/<channel>/artifacts/`
- set `--slack-artifact-retention-days 0` to disable artifact expiration

Inspect or repair ChannelStore state for a specific channel:

```bash
# Inspect one channel
cargo run -p pi-coding-agent -- \
  --channel-store-root .pi/channel-store \
  --channel-store-inspect github/issue-9

# Repair malformed log/context JSONL lines for one channel
cargo run -p pi-coding-agent -- \
  --channel-store-root .pi/channel-store \
  --channel-store-repair slack/C123
```

Inspect now reports artifact health counters (records, invalid index lines, active/expired), and repair also purges expired artifacts plus invalid artifact index rows.

Validate an extension manifest JSON and exit:

```bash
cargo run -p pi-coding-agent -- \
  --extension-validate .pi/extensions/issue-assistant/extension.json
```

Inspect extension manifest metadata and declared inventory:

```bash
cargo run -p pi-coding-agent -- \
  --extension-show .pi/extensions/issue-assistant/extension.json
```

List discovered extension manifests from an extension root (including invalid entries):

```bash
cargo run -p pi-coding-agent -- \
  --extension-list \
  --extension-list-root .pi/extensions
```

Execute one declared process-runtime extension hook with a JSON payload:

```bash
cargo run -p pi-coding-agent -- \
  --extension-exec-manifest .pi/extensions/issue-assistant/extension.json \
  --extension-exec-hook run-start \
  --extension-exec-payload-file .pi/extensions/issue-assistant/payload.json
```

Enable runtime lifecycle hook dispatch (`run-start`, `run-end`, `pre-tool-call`, `post-tool-call`) for prompt turns:

```bash
cargo run -p pi-coding-agent -- \
  --model openai/gpt-4o-mini \
  --openai-api-key "$OPENAI_API_KEY" \
  --prompt "Summarize open issues" \
  --extension-runtime-hooks \
  --extension-runtime-root .pi/extensions
```

For `message-transform` hooks, extension responses can return a replacement prompt via `{"prompt":"..."}`.

When `--extension-runtime-hooks` is enabled, process extensions can also declare runtime tools and slash commands:

```json
{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "runtime.sh",
  "permissions": ["run-commands"],
  "tools": [
    {
      "name": "issue_triage",
      "description": "Classify and label issues",
      "parameters": {
        "type": "object",
        "properties": {
          "title": { "type": "string" }
        },
        "required": ["title"],
        "additionalProperties": false
      }
    }
  ],
  "commands": [
    {
      "name": "/triage-now",
      "description": "Run triage pipeline",
      "usage": "/triage-now <issue-number>"
    }
  ]
}
```

Runtime tool response contract:
- return JSON object with `content` (any JSON) and optional `is_error` boolean.

Runtime slash-command response contract:
- return JSON object with optional `output` (or `message`) string and optional `action` of `continue` or `exit`.

Validate a reusable package manifest JSON and exit:

```bash
cargo run -p pi-coding-agent -- \
  --package-validate .pi/packages/starter/package.json
```

Inspect package manifest metadata and component inventory:

```bash
cargo run -p pi-coding-agent -- \
  --package-show .pi/packages/starter/package.json
```

Install a local package manifest bundle into a versioned package root:

```bash
cargo run -p pi-coding-agent -- \
  --package-install ./examples/starter/package.json \
  --package-install-root .pi/packages
```

Package components can also source content from remote HTTP(S) URLs with optional checksum pinning:

```json
{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [
    {
      "id": "review",
      "path": "templates/review.txt",
      "url": "https://example.com/templates/review.txt",
      "sha256": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    }
  ]
}
```

Require package signatures during install (trusted keys come from skill trust roots):

```bash
cargo run -p pi-coding-agent -- \
  --package-install ./examples/starter/package.json \
  --package-install-root .pi/packages \
  --require-signed-packages \
  --skill-trust-root publisher=BASE64_PUBLIC_KEY
```

Update an already installed package bundle from a manifest (fails if target version is not installed):

```bash
cargo run -p pi-coding-agent -- \
  --package-update ./examples/starter/package.json \
  --package-update-root .pi/packages
```

Audit installed package bundles for deterministic component path conflicts:

```bash
cargo run -p pi-coding-agent -- \
  --package-conflicts \
  --package-conflicts-root .pi/packages
```

Materialize installed package components into an activation destination with explicit conflict policy:

```bash
cargo run -p pi-coding-agent -- \
  --package-activate \
  --package-activate-root .pi/packages \
  --package-activate-destination .pi/packages-active \
  --package-activate-conflict-policy keep-first
```

Activate installed package components during normal startup (without exiting), then run with activated package skills:

```bash
cargo run -p pi-coding-agent -- \
  --package-activate-on-startup \
  --package-activate-root .pi/packages \
  --package-activate-destination .pi/packages-active \
  --package-activate-conflict-policy keep-first \
  --skill checks
```

List installed package bundles from a package root:

```bash
cargo run -p pi-coding-agent -- \
  --package-list \
  --package-list-root .pi/packages
```

Remove one installed package bundle from a package root:

```bash
cargo run -p pi-coding-agent -- \
  --package-remove starter-bundle@1.0.0 \
  --package-remove-root .pi/packages
```

Rollback one package to a target installed version (removes sibling versions):

```bash
cargo run -p pi-coding-agent -- \
  --package-rollback starter-bundle@1.0.0 \
  --package-rollback-root .pi/packages
```

Print versioned RPC protocol capabilities JSON and exit:

```bash
cargo run -p pi-coding-agent -- \
  --rpc-capabilities
```

Validate one RPC frame JSON request/command shape and exit:

```bash
cargo run -p pi-coding-agent -- \
  --rpc-validate-frame-file /tmp/rpc-frame.json
```

Dispatch one RPC request frame JSON into a response frame JSON and exit:

```bash
cargo run -p pi-coding-agent -- \
  --rpc-dispatch-frame-file /tmp/rpc-frame.json
```

RPC request frame schema versions `0` and `1` are accepted; response frames are emitted with schema version `1`. `capabilities.request` accepts optional `request_schema_version` for explicit negotiation, and `capabilities.response` includes `response_schema_version`, `supported_request_schema_versions`, `negotiated_request_schema_version`, `contracts.run_status` metadata (terminal flag contract + serve closed-status retention capacity + declared `status_values`/`terminal_states`), `contracts.errors.codes` taxonomy metadata (machine-readable RPC error contract list), and `contracts.protocol` kind inventories (`request_kinds`, `response_kinds`, `stream_event_kinds`).
Schema compatibility fixture replay coverage for dispatch/serve mode lives under `crates/pi-coding-agent/testdata/rpc-schema-compat/`.

For invalid request frames, dispatch still prints a structured JSON error envelope (`kind: "error"` with `payload.code` and `payload.message`) and exits non-zero.

Dispatch newline-delimited RPC request frames (NDJSON) and emit one response JSON line per input frame:

```bash
cargo run -p pi-coding-agent -- \
  --rpc-dispatch-ndjson-file /tmp/rpc-frames.ndjson
```

Run long-lived RPC NDJSON server mode over stdin/stdout:

```bash
cat /tmp/rpc-frames.ndjson | cargo run -p pi-coding-agent -- --rpc-serve-ndjson
```

In `--rpc-serve-ndjson` mode, run lifecycle is stateful: `run.start` returns a `run_id` and emits deterministic `run.stream.tool_events` plus `run.stream.assistant_text` frames, `run.status` reports active/inactive state for that `run_id`, `run.complete` closes active runs with `run.completed`, `run.fail` closes active runs with `run.failed`, `run.timeout` closes active runs with `run.timed_out`, and `run.cancel` closes active runs with `run.cancelled` plus terminal `run.stream.tool_events` and terminal `run.stream.assistant_text` frames (unknown `run_id` still returns a structured `error` envelope).
Terminal lifecycle envelopes (`run.cancelled`, `run.completed`, `run.failed`, `run.timed_out`) include explicit `terminal` and `terminal_state` payload fields; terminal serve-mode stream tool events and assistant_text frames mirror the same terminal metadata (`final: true` on assistant_text).
In serve mode, `run.status` also retains closed-run terminal outcomes for known `run_id` values (`known: true`, `active: false`, terminal metadata, and `reason` when applicable). `run.status` payloads always include an explicit `terminal` flag (`false` for active/unknown, `true` for closed terminal states). Closed-run retention is bounded to a fixed in-memory window to keep long-lived sessions stable.

Run the autonomous events scheduler (immediate, one-shot, periodic):

```bash
cargo run -p pi-coding-agent -- \
  --model openai/gpt-4o-mini \
  --events-runner \
  --events-dir .pi/events \
  --events-state-path .pi/events/state.json \
  --events-poll-interval-ms 1000 \
  --events-queue-limit 64
```

Queue a webhook-triggered immediate event from a payload file (debounced):

```bash
cargo run -p pi-coding-agent -- \
  --events-dir .pi/events \
  --events-state-path .pi/events/state.json \
  --event-webhook-ingest-file /tmp/webhook.json \
  --event-webhook-channel slack/C123 \
  --event-webhook-prompt-prefix "Handle incoming deployment signal." \
  --event-webhook-debounce-key deploy-hook \
  --event-webhook-debounce-window-seconds 60
```

Queue a signed webhook payload with verification and replay protection:

```bash
cargo run -p pi-coding-agent -- \
  --events-dir .pi/events \
  --events-state-path .pi/events/state.json \
  --event-webhook-ingest-file /tmp/webhook.json \
  --event-webhook-channel github/owner/repo#42 \
  --event-webhook-signature "$X_HUB_SIGNATURE_256" \
  --event-webhook-secret "$WEBHOOK_SECRET" \
  --event-webhook-signature-algorithm github-sha256 \
  --event-webhook-signature-max-skew-seconds 300
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

Warm the remote skill cache, then replay installs offline:

```bash
# Online warm-cache run
cargo run -p pi-coding-agent -- \
  --prompt "Audit this module" \
  --skills-dir .pi/skills \
  --skills-cache-dir .pi/skills-cache \
  --install-skill-url https://example.com/skills/review.md \
  --install-skill-sha256 2f7d0... \
  --skill review

# Offline replay run (no remote fetches; requires cache hit)
cargo run -p pi-coding-agent -- \
  --prompt "Audit this module" \
  --skills-dir .pi/skills \
  --skills-cache-dir .pi/skills-cache \
  --skills-offline \
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

Replay registry installs offline from cache:

```bash
cargo run -p pi-coding-agent -- \
  --prompt "Audit this module" \
  --skills-dir .pi/skills \
  --skills-cache-dir .pi/skills-cache \
  --skills-offline \
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
/session-stats --json
/session-diff
/session-diff 12 24
/doctor
/doctor --json
/session-graph-export /tmp/session-graph.mmd
/branches

# Save/list/run repeatable command macros (project-local .pi/macros.json)
/macro save quick-check /tmp/quick-check.commands
/macro list
/macro show quick-check
/macro run quick-check --dry-run
/macro run quick-check
/macro delete quick-check

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

# Persist and use named bookmarks for investigation checkpoints
/session-bookmark set investigation 12
/session-bookmark list
/session-bookmark use investigation
/session-bookmark delete investigation

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

# Mutate trust-root keys interactively (uses configured trust-root path by default)
/skills-trust-add root-v2=AbC...
/skills-trust-revoke root-v1
/skills-trust-rotate root-v1:root-v2=AbC...

# List currently installed skills
/skills-list

# Write/update skills lockfile from currently installed skills
/skills-lock-write
/skills-lock-write /tmp/custom-skills.lock.json

# Validate installed skills against lockfile drift without restarting
/skills-sync
/skills-sync /tmp/custom-skills.lock.json

# Run combined lockfile + trust/signature compliance diagnostics
/skills-verify
/skills-verify /tmp/custom-skills.lock.json /tmp/trust-roots.json --json

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
