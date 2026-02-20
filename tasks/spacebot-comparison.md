# Spacebot vs Tau — Detailed Comparison & Gap Analysis

**Date**: 2026-02-17
**Spacebot**: v0.1.3 (Feb 17, 2026) — https://github.com/spacedriveapp/spacebot
**Tau**: HEAD `1d488237` — 243,968 lines, 42 crates

---

## What Spacebot Is

Spacebot is a Rust-based AI agent for teams, communities, and multi-user environments. Its core innovation is a **five-process-type architecture** that splits a monolithic agent into concurrent specialized processes: Channels (conversation), Branches (thinking), Workers (task execution), Compactor (context management), and Cortex (system observation). This eliminates the single-session bottleneck — 50 users can interact simultaneously.

- **Stars**: 145 | **Language**: Rust | **License**: FSL-1.1-ALv2 (→ Apache 2.0 after 2 years)
- **Created**: February 11, 2026 (6 days old)
- **By**: Spacedrive team (the open-source file manager)

---

## Architecture Comparison

| Dimension | Spacebot | Tau |
|-----------|----------|-----|
| **Core model** | 5 concurrent process types (channel, branch, worker, compactor, cortex) | Single agent loop with tool delegation |
| **Concurrency** | Multiple users served simultaneously, never blocking | Single-threaded turn loop per session |
| **Context management** | Tiered compaction (80/85/95% thresholds) with LLM summarization | Session DAG with branch/merge, no automatic compaction |
| **Memory** | Typed graph (8 types, 6 edge relations) + hybrid search (vector + FTS + graph via RRF) | Hybrid search (BM25 + vector via RRF) + namespace tree |
| **Thinking** | Branches fork channel context, think independently, return conclusions | No separate thinking process — agent thinks inline |
| **Task execution** | Workers with fire-and-forget or interactive modes, 25-turn segments | Jobs system with background tokio tasks |
| **Observation** | Cortex monitors all channels/workers, generates memory bulletins | No cross-session observer |
| **Embeddings** | Local FastEmbed (no API calls) | Provider API (OpenAI text-embedding-3-small) with FNV1a fallback |
| **DB architecture** | 3 DBs: SQLite (relational) + LanceDB (vector/FTS) + redb (KV/secrets) | SQLite for sessions/memory, JSONL fallback, PostgreSQL |
| **Prompts** | Jinja2 templates (minijinja) | Hardcoded system prompts in Rust |
| **Config** | TOML with hot-reload (arc-swap + notify file watching) | Profile TOML, CLI args, env vars. No hot-reload |
| **Web UI** | React 19 SPA with memory graph visualization, cortex admin chat | Gateway webchat shell, dashboard health/metrics endpoints |
| **Identity** | SOUL.md, IDENTITY.md, USER.md injected into prompts | SOUL.md, AGENTS.md, USER.md via tau-onboarding |
| **Binary** | Single binary, daemon mode (start/stop/restart/status) | Single binary, gateway mode, transport bridges |
| **Codebase size** | ~15-20K lines (estimated from file sizes) | 243,968 lines across 42 crates |

---

## Feature-by-Feature Comparison

### Channels & Communication

| Feature | Spacebot | Tau | Gap? |
|---------|----------|-----|------|
| Discord | Full (Serenity): threads, reactions, file uploads, streaming via edits, mention resolution, message splitting | No | **YES** |
| Slack | Full (slack-morphism): Socket Mode, file uploads, streaming, threads, reactions | Full (tau-slack-runtime): Socket Mode | No |
| Telegram | Basic (teloxide) | Full (tau-multi-channel) | No |
| WhatsApp | No | Yes (tau-multi-channel) | Tau ahead |
| GitHub Issues | No | Yes (tau-github-issues-runtime) | Tau ahead |
| HTTP Webhook | Yes (receive) | Yes (gateway, send+receive) | No |
| WebSocket | No dedicated WS transport | Yes (gateway /gateway/ws) | Tau ahead |
| CLI/REPL | No (daemon + API only) | Yes (primary interface) | Tau ahead |
| Message coalescing | Yes — batches rapid-fire messages into single LLM turn | No | **YES** |
| Streaming responses | Via message edits (Discord/Slack) | Via SSE streaming (gateway) | Different approaches |
| Cross-channel awareness | Yes — Cortex sees all channels | No — sessions are isolated | **YES** |

### Memory System

| Feature | Spacebot | Tau | Gap? |
|---------|----------|-----|------|
| Memory types | 8 typed (Identity, Goal, Decision, Todo, Preference, Fact, Event, Observation) | Untyped key-value with namespace tree | **YES** |
| Importance scoring | Per-memory importance (0.0-1.0), type-based defaults | No importance scoring | **YES** |
| Importance decay | Time-based decay on unaccessed memories (Identity exempt) | No decay | **YES** |
| Graph relations | 6 edge types (RelatedTo, Updates, Contradicts, CausedBy, ResultOf, PartOf) | No graph structure | **YES** |
| Graph traversal in search | Yes — BFS from high-importance seeds with keyword matching | No | **YES** |
| Vector search | LanceDB with HNSW index + local FastEmbed | Provider API embeddings | Partial — Tau has vectors but depends on API |
| Full-text search | LanceDB Tantivy | BM25 | No (different implementations, similar capability) |
| Hybrid search (RRF) | Vector + FTS + Graph (3-way) | BM25 + Vector (2-way) | **YES** — Tau missing graph dimension |
| Near-duplicate merging | Yes | No | **YES** |
| Memory pruning | Yes — below importance floor | No | **YES** |
| Orphan detection | Yes | No | **YES** |
| Soft delete | Yes (forgotten flag, excluded from search) | No | **YES** |
| Local embeddings | FastEmbed (no API cost) | No — requires provider API call | **YES** |
| Memory ingestion (bulk) | Yes — watch directory, chunk files, LLM extraction | No | **YES** |
| Compaction-initiated saves | Yes — compactor extracts memories during summarization | No | **YES** |

### Agent Process Architecture

| Feature | Spacebot | Tau | Gap? |
|---------|----------|-----|------|
| Branch (forked thinking) | Yes — git-like fork of context, runs independently | Session DAG branching (manual, not LLM-initiated) | **YES** |
| Workers (task execution) | Yes — fire-and-forget + interactive, 25-turn segments | Jobs system (background tokio tasks) | Partial |
| Compaction (context management) | Yes — tiered (80/85/95%), background LLM summarization | No automatic compaction | **YES** |
| Cortex (system observer) | Yes — cross-channel, memory bulletins, admin chat | No equivalent | **YES** |
| Message coalescing | Yes — batches rapid messages | No | **YES** |
| Context overflow recovery | Yes — up to 3 compaction retries | No | **YES** |
| Process supervision | Cortex monitors all processes | No centralized supervisor | **YES** |

### Tools

| Tool | Spacebot | Tau | Gap? |
|------|----------|-----|------|
| reply (message user) | Yes | Agent output is the reply | Different model |
| branch (fork thinking) | Yes (tool) | Session branching (not agent-initiated) | **YES** |
| spawn_worker | Yes (tool) | jobs_create (similar) | No |
| route (to worker) | Yes (send follow-up to interactive worker) | sessions_send (similar) | No |
| cancel | Yes (cancel worker/branch) | jobs_cancel | No |
| skip (opt out of responding) | Yes | No | **YES** |
| react (emoji) | Yes | No | **YES** |
| cron (scheduled tasks) | Yes (tool) | Routine engine (cron, events, webhooks) | No |
| send_file | Yes | No direct file sending tool | **YES** |
| memory_recall | Yes (hybrid search + curate) | memory_search | No |
| memory_save | Yes (typed, with importance) | memory_write | Partial — no typing/importance |
| memory_delete | Yes (soft delete) | No | **YES** |
| channel_recall | Yes (cross-channel transcript) | No | **YES** |
| shell | Yes (configurable timeout) | bash (allowlisted, sandboxed) | No |
| file (read/write/list) | Yes | read, write, edit (3 tools) | No |
| exec (subprocess) | Yes | bash covers this | No |
| set_status | Yes (worker visible status) | No | **YES** |
| browser (headless Chrome/CDP) | Yes (chromiumoxide, accessibility tree refs) | Hybrid scaffold (std::process::Command) | **YES** |
| web_search (Brave API) | Yes | http tool (generic) | **YES** — no dedicated search |
| tool_builder (WASM) | No | Yes | Tau ahead |
| http (generic HTTP) | No (only web_search) | Yes | Tau ahead |
| undo/redo | No | Yes | Tau ahead |
| sessions tools (list/history/search/stats) | No | Yes (5 tools) | Tau ahead |

### LLM Provider & Routing

| Feature | Spacebot | Tau | Gap? |
|---------|----------|-----|------|
| Provider count | 11 (Anthropic, OpenAI, OpenRouter, Z.ai, Groq, Together, Fireworks, DeepSeek, xAI, Mistral, OpenCode Zen) | 4 direct + 6 aliases (OpenAI, OpenRouter, Anthropic, Google + deepseek, groq, xai, mistral, azure, azure-openai) | Partial — Tau missing Together, Fireworks, Z.ai, OpenCode Zen |
| Process-type defaults | Yes — different models per process type | No — one model for everything | **YES** |
| Task-type overrides | Yes — coding tasks upgrade to better model | No | **YES** |
| Prompt complexity scoring | Yes — keyword-based light/standard/heavy classification | No | **YES** |
| Fallback chains | Yes (3 retries + 3 fallbacks) | Yes (circuit breaker + fallback chain) | No |
| Rate limit cooldown | Yes (60s cooldown for 429'd models) | Yes (30s circuit breaker cooldown) | No |
| Model catalog | No centralized catalog | Yes — 35 models with full metadata | Tau ahead |
| Remote catalog discovery | No | Yes — OpenRouter API fetch + merge | Tau ahead |
| Prompt caching | No | Yes (Anthropic cache_control, Google cached_content) | Tau ahead |
| Token pre-flight | No | Yes (model-aware preflight ceilings) | Tau ahead |
| Per-session cost tracking | No | Yes (SessionUsageRecord) | Tau ahead |

### Security

| Feature | Spacebot | Tau | Gap? |
|---------|----------|-----|------|
| Secret leak detection | Yes — regex scan on tool args (block) and results (terminate) | Yes — Aho-Corasick + regex (warn/redact/block) | No |
| Encrypted secrets | Yes — AES-256-GCM in redb | No encrypted secret store | **YES** |
| Tool output truncation | Yes — 50KB cap | Yes — tool policy limits | No |
| Workspace path guards | Yes — reject writes to identity/memory paths | Yes — protected paths | No |
| Sandbox execution | No | Yes — bubblewrap, Docker, WASM fuel metering | Tau ahead |
| SSRF protection | No | Yes — RFC 1918, metadata endpoint blocking | Tau ahead |
| Tool policy tiers | No (all tools available) | Yes — 4 tiers (Permissive/Balanced/Strict/Hardened) | Tau ahead |
| RBAC | No | Yes — per-principal authorization | Tau ahead |

### Web UI & Dashboard

| Feature | Spacebot | Tau | Gap? |
|---------|----------|-----|------|
| Full SPA | Yes — React 19, TanStack Router, Tailwind, Framer Motion | No — webchat shell + API endpoints | **YES** |
| Memory graph visualization | Yes — Sigma.js + Graphology force-directed graph | No | **YES** |
| Cortex admin chat | Yes — real-time SSE streaming | No | **YES** |
| Channel conversation viewer | Yes — timeline with worker/branch status | No | **YES** |
| Cron job management UI | Yes | No | **YES** |
| Agent configuration editor | Yes | No (CLI/TOML only) | **YES** |
| Memory browser with search | Yes | No | **YES** |
| File ingestion UI | Yes — drag-and-drop upload | No | **YES** |
| Code display | Yes — CodeMirror | No | **YES** |

### Operations & Deployment

| Feature | Spacebot | Tau | Gap? |
|---------|----------|-----|------|
| Daemon mode (start/stop/restart/status) | Yes | Gateway mode (persistent server) | Partial |
| Docker images | Yes — slim + full (with Chromium), multi-arch | Yes — Dockerfile, GHCR | No |
| Fly.io deployment | Yes — fly.toml configured | No | **YES** |
| Hosted SaaS | Yes — spacebot.sh with pricing tiers | No | **YES** |
| Hot-reload config | Yes — arc-swap + notify file watcher | No | **YES** |
| Homebrew | No | Yes | Tau ahead |
| Shell completions | No | Yes (bash/zsh/fish) | Tau ahead |
| Systemd unit | No | Yes (tau-ops daemon) | Tau ahead |
| Multi-platform releases | No (Docker only) | Yes — 6 platform native binaries | Tau ahead |
| CI release pipeline | Yes (Docker + Fly.io rollout) | Yes (native binaries + checksums + attestation) | No |

### Unique to Spacebot (Not in Tau)

| Feature | Description |
|---------|-------------|
| **OpenCode integration** | Persistent subprocess for deep coding sessions via HTTP+SSE |
| **Skills system** | SKILL.md files (markdown + YAML frontmatter), OpenClaw-compatible |
| **Memory ingestion** | Watch directory, auto-chunk text files, LLM-extract memories |
| **Cortex bulletins** | LLM-curated system briefing injected into every channel |
| **Documentation site** | Full Next.js docs site (fumadocs) with guides and design docs |
| **Template prompts** | Jinja2 templates (minijinja) — user-editable, not hardcoded |

### Unique to Tau (Not in Spacebot)

| Feature | Description |
|---------|-------------|
| **42-crate modular architecture** | Deeply decomposed, independently testable |
| **WASM runtime** | wasmtime with fuel metering, self-building tools |
| **Session DAG** | Graph-based sessions with branch/merge/undo/redo |
| **PPO/GAE training** | Reinforcement learning building blocks wired into training loop |
| **Prompt optimization** | APO beam search over prompt variants |
| **MCP client/server** | Full Model Context Protocol bidirectional support |
| **KAMN identity** | DID-based decentralized agent identity, reputation, economic coordination |
| **Multi-agent orchestration** | Plan-first mode, capability-based routing, delegation |
| **Tool policy system** | 4-tier security with per-tool rate limits and sandbox requirements |
| **OpenAI Responses API** | Dual-API support with Codex model detection |
| **Live validation harness** | Repeatable multi-provider conformance testing |
| **2,573 tests** | Extensive test coverage |

---

## Gap Inventory: What Spacebot Has That Tau Doesn't

### Tier 1 — Architectural (Requires Design Work)

#### G1. Multi-Process Agent Architecture
**What**: Spacebot's 5-process model (channel, branch, worker, compactor, cortex) vs Tau's single turn loop.
**Why it matters**: True multi-user support. One user's complex request doesn't block another user's simple question.
**Pathway**:
- [x] Design a `ProcessType` enum: `Channel`, `Branch`, `Worker`, `Compactor`, `Cortex`
- [x] Refactor `tau-agent-core` turn loop to support concurrent process instances
- [x] Add `ProcessManager` that spawns/supervises process lifecycles
- [x] Each process gets its own system prompt, tool set, and context window
- [x] Channel process delegates to branches/workers via tool calls
- [x] Worker process runs in isolated tokio task with 25-turn segments
- **Files**: `tau-agent-core/src/lib.rs`, new `tau-agent-core/src/process_types/` module
- **Effort**: Large — this is an architectural change to the core agent loop

#### G2. Automatic Context Compaction
**What**: Spacebot monitors context size and triggers tiered compaction (80% → background summarize, 85% → aggressive, 95% → emergency truncation).
**Why it matters**: Long conversations eventually exceed context windows. Without compaction, the agent crashes or loses history.
**Pathway**:
- [x] Phase 3 (`#2566`): implement non-blocking warn-tier background compaction scheduling with deterministic apply/fallback on subsequent turns
- [x] Add `ContextMonitor` to `tau-agent-core` that tracks token count per session
- [x] Implement 3-tier thresholds (configurable in profile TOML)
- [x] At 80%: spawn background compaction task that summarizes oldest 30% of context via LLM call
- [x] At 85%: aggressive compaction (50% of context)
- [x] At 95%: emergency hard truncation (drop oldest 50%, no LLM)
- [x] During compaction, extract and save memories (like Spacebot's compactor)
- [x] Compaction summaries stored as session entries (encoded as deterministic `[Tau compaction entry]` system artifacts)
- **Files**: New `tau-agent-core/src/compaction.rs`, modify turn loop to check thresholds
- **Effort**: Medium

#### G3. Cortex (Cross-Session Observer)
**What**: A background process that sees across all channels/sessions, generates memory bulletins, detects patterns, and provides admin chat.
**Why it matters**: Without it, each session is an island. The agent can't learn from one conversation and apply it to another.
**Pathway**:
- [x] Create `Cortex` struct in `tau-agent-core` or new `tau-cortex` crate
- [x] Runs on heartbeat interval (already exists in tau-runtime)
- [x] Queries memory store across all sessions
- [x] Generates periodic "memory bulletin" via LLM summarization
- [x] Bulletin injected into all new session system prompts via `ArcSwap<String>`
- [x] Add admin chat endpoint in gateway (`POST /cortex/chat` returning SSE)
- [x] Add cortex observer status endpoint in gateway (`GET /cortex/status` with deterministic counters/fallback diagnostics)
- [x] Track core gateway cortex observer events (`cortex.chat.request`, `session.append`, `session.reset`, `external_coding_agent.session_opened`, `external_coding_agent.session_closed`)
- [x] Track cortex events (memory saves, session starts/ends, worker completions)
- **Files**: New crate or module, modify `tau-gateway` for admin endpoints
- **Effort**: Large

#### G4. Branch-as-Tool (LLM-Initiated Forked Thinking)
**What**: The agent can invoke a `branch` tool to fork its context and think independently, returning only the conclusion.
**Why it matters**: Separates "thinking" from "talking". The user never sees the agent's working — only the answer. Reduces noise.
**Pathway**:
- [x] Add `BranchTool` to `tau-tools` that creates a new session branch with the current context
- [x] Branch runs a separate agent turn with memory tools but no user-facing reply tools
- [x] Branch returns a structured conclusion to the parent session
- [x] Parent session receives conclusion as a tool result and can act on it
- [x] Configurable max concurrent branches per session
- **Files**: `tau-tools/src/tools/`, `tau-agent-core/src/` for branch execution
- **Effort**: Medium

### Tier 2 — Memory Enhancements (Extends Existing System)

#### G5. Typed Memories with Importance Scoring
**What**: 8 memory types (Identity, Goal, Decision, Todo, Preference, Fact, Event, Observation) with per-type default importance (0.3-1.0).
**Why it matters**: Not all memories are equal. "User's name is Alice" (Identity, 1.0) should never decay. "Discussed weather today" (Observation, 0.3) should.
**Pathway**:
- [x] Add `MemoryType` enum to `tau-memory`: `Identity`, `Goal`, `Decision`, `Todo`, `Preference`, `Fact`, `Event`, `Observation`
- [x] Add `importance` field to memory entries
- [x] Default importance per type configurable via profile/runtime
- [x] Modify `memory_write` tool to accept type and optional importance override
- [x] Modify `memory_search` to boost by importance in ranking
- **Files**: `tau-memory/src/`, `tau-tools/src/tools/memory_tools.rs`
- **Effort**: Small-Medium

#### G6. Memory Graph Relations
**What**: 6 edge types connecting memories (RelatedTo, Updates, Contradicts, CausedBy, ResultOf, PartOf) with weighted graph traversal in search.
**Why it matters**: Enables the agent to follow chains of reasoning. "Decision X" → `CausedBy` → "Event Y" → `ResultOf` → "Goal Z".
**Pathway**:
- [x] Add relation table to tau-memory SQLite schema (`memory_relations` with source/target/relation_type/weight/effective_weight)
- [x] Add `MemoryRelation` enum with Spacebot-parity 6 relation types
- [x] Modify `memory_save` to accept optional `relates_to` parameter
- [x] Add graph BFS traversal to memory search
- [x] Merge graph results into RRF scoring
- **Files**: `tau-memory/src/`, memory search functions
- **Effort**: Medium

#### G7. Memory Lifecycle (Decay, Pruning, Dedup, Orphan Cleanup)
**What**: Importance decay on unaccessed memories, pruning below floor, near-duplicate merging, orphan detection.
**Why it matters**: Without lifecycle management, memory grows unbounded and search quality degrades.
**Pathway**:
- [x] Add `last_accessed_at` and `access_count` to memory entries
- [x] Add decay function on heartbeat: `importance *= decay_rate` for memories not accessed in N days (Identity exempt)
- [x] Prune memories below configurable importance floor (default 0.1)
- [x] Near-duplicate detection via cosine similarity threshold on embeddings
- [x] Orphan cleanup: memories with no graph edges and low importance
- [x] Soft delete via `forgotten` flag (excluded from search, retained in DB)
- **Files**: `tau-memory/src/`, add maintenance task to heartbeat
- **Effort**: Medium

#### G8. Local Embeddings (FastEmbed)
**What**: Spacebot uses FastEmbed for local embedding generation — no API calls, no cost, no latency.
**Why it matters**: Every memory save/search in Tau requires an API call. Local embeddings are free and fast.
**Pathway**:
- [x] Add `fastembed` as workspace dependency
- [x] Create `LocalEmbeddingProvider` implementing the embedding trait
- [x] Configure via profile: `embedding_provider = "local"` or `"openai"`
- [x] Make `embedding_provider = "local"` the default profile setting
- [x] Local model: `BAAI/bge-small-en-v1.5` or similar (same as Spacebot's default)
- [x] Fall back to FNV1a hash only if local model fails to load
- **Files**: `tau-memory/src/`, `Cargo.toml`
- **Effort**: Small

#### G9. Memory Ingestion (Bulk Import)
**What**: Watch a directory, auto-process text files (txt, md, json, csv, etc.), chunk and extract memories via LLM.
**Why it matters**: Users can dump knowledge bases, docs, or exported data and the agent absorbs it.
**Pathway**:
- [x] Add `IngestionWorker` to `tau-memory` or new module
- [x] Watch `{workspace}/ingest/` directory via `notify` crate or heartbeat polling
- [x] Chunk files at line boundaries (configurable chunk size)
- [x] Process each chunk through LLM with memory_save tool
- [x] Track progress per-chunk in SQLite (SHA-256 content hash) for crash resilience
- [x] Delete files after successful ingestion
- [x] Support: txt, md, json, jsonl, csv, tsv, log, xml, yaml, toml
- **Files**: New module in `tau-memory` or `tau-runtime`
- **Effort**: Medium

### Tier 3 — Communication Enhancements

#### G10. Discord Adapter
**What**: Full Discord integration with Serenity — threads, reactions, file uploads, streaming via edits, mention resolution.
**Why it matters**: Discord is the dominant community platform. Spacebot's primary deployment target.
**Pathway**:
- [ ] Add `serenity` as workspace dependency
- [ ] Create `tau-discord-runtime` crate or add to `tau-multi-channel`
- [ ] Implement: message send/receive, file attachments, thread creation, emoji reactions, typing indicators
- [ ] Message streaming via placeholder message + progressive edits
- [x] Message history backfill (up to 100 messages before trigger) (`#2758`)
- [x] Mention resolution (`<@ID>` / `<@!ID>` → `@DisplayName`) (`#2662`)
- [x] Auto-split messages at 2000 char limit (`#2662`)
- [x] Guild/channel filtering for permissions (`#2750`)
- **Files**: New crate or `tau-multi-channel/src/discord.rs`
- **Effort**: Medium-Large

#### G11. Message Coalescing
**What**: When users send multiple rapid messages, batch them into a single LLM turn instead of processing each separately.
**Why it matters**: Users often split thoughts across multiple messages. Without coalescing, the agent responds to "Hey" before seeing "can you help me with X?"
**Pathway**:
- [x] Add coalescing buffer to channel/session inbound message handling
- [x] Configurable window (default 2-3 seconds)
- [x] If another message arrives within the window, extend and batch
- [x] Join batched messages with newlines before dispatching to agent
- [x] Typing indicator during coalescing window
- **Files**: `tau-agent-core/src/`, channel runtime modules
- **Effort**: Small

#### G12. Skip Tool (Opt-Out of Responding)
**What**: The agent can explicitly choose not to respond to a message, with a logged reason.
**Why it matters**: In multi-user channels, the agent shouldn't respond to every message. It needs judgment about when to speak.
**Pathway**:
- [x] Add `SkipTool` to `tau-tools` — takes a `reason: String` parameter
- [x] When invoked, the turn ends without sending any output to the user
- [x] Log the skip reason for debugging/tuning
- [x] Include in channel/multi-user tool sets
- **Files**: `tau-tools/src/tools/`
- **Effort**: Small

#### G13. React Tool (Emoji Reactions)
**What**: The agent can add emoji reactions to messages instead of replying.
**Why it matters**: Lightweight acknowledgment without cluttering the conversation. "Got it" as a thumbs-up.
**Pathway**:
- [x] Add `ReactTool` to `tau-tools` — takes `message_id` and `emoji` parameters
- [x] Wire to platform adapters (Slack, Discord, Telegram all support reactions)
- [x] Channel adapter translates emoji to platform-specific format
- **Files**: `tau-tools/src/tools/`, messaging runtime modules
- **Effort**: Small

#### G14. Send File Tool
**What**: Agent can send file attachments to users.
**Why it matters**: Code generation, reports, images — the agent needs to deliver artifacts, not just text.
**Pathway**:
- [x] Add `SendFileTool` to `tau-tools` — takes `file_path` and optional `message` parameters
- [x] Wire to platform adapters (Discord file upload, Slack v2 upload, Telegram sendDocument)
- [x] Gateway: return file as attachment in response
- **Files**: `tau-tools/src/tools/`, messaging runtime modules
- **Effort**: Small

### Tier 4 — Routing & Configuration

#### G15. Process-Type Model Routing
**What**: Different LLM models for different process types. Cheap model for simple replies, expensive model for coding tasks.
**Why it matters**: Cost optimization. A simple "hello" reply doesn't need Claude Opus 4.6.
**Pathway**:
- [x] Add `routing` section to profile TOML: `channel_model`, `branch_model`, `worker_model`, `compactor_model`, `cortex_model`
- [x] Add `task_overrides` map: `coding → expensive_model`, `summarization → cheap_model`
- [x] Implement prompt complexity scoring (keyword-based, <1ms, no API calls): classify messages as light/standard/heavy
- [x] Apply overrides at LLM client dispatch time
- **Files**: `tau-provider/src/`, profile config, `tau-agent-core/src/`
- **Effort**: Medium

#### G16. Hot-Reload Configuration
**What**: Config changes take effect without restart. Spacebot uses `arc-swap` + `notify` file watcher.
**Why it matters**: In production, restarting drops all active sessions. Config changes should be seamless.
**Pathway**:
- [x] Phase 3 (`#2541`): bridge active profile store heartbeat interval updates into runtime heartbeat `.policy.toml` hot-reload payloads with deterministic `applied/no_change/invalid/missing_profile` diagnostics
- [x] Add `notify` as workspace dependency (file system watcher)
- [x] Wrap config in `ArcSwap<Config>` — atomic pointer swap, lock-free reads
- [x] Watch profile TOML for changes
- [x] On change: parse new config, validate, swap atomically
- [x] Log config reloads
- **Files**: `tau-onboarding/src/`, `tau-runtime/src/`
- **Effort**: Small-Medium

#### G17. Jinja2 Template Prompts
**What**: System prompts as Jinja2 templates that users can edit without recompiling.
**Why it matters**: Hardcoded prompts require code changes to customize. Templates let operators tune behavior.
**Pathway**:
- [x] Add `minijinja` as workspace dependency (`#2482`)
- [x] Move system prompts from Rust string literals to `.md.j2` template files (`#2471`)
- [x] Load from `{workspace}/prompts/` with built-in defaults (`#2476`)
- [x] Template variables: `{identity}`, `{memory_bulletin}`, `{tools}`, `{active_workers}`, etc. (`#2482`)
- [x] Hot-reload templates when files change (combine with G16) (`#2548`)
- **Files**: New `prompts/` directory, `tau-agent-core/src/`
- **Effort**: Medium

### Tier 5 — UI & Operational

#### G18. Full Web Dashboard (React SPA)
**What**: Spacebot has a complete React SPA with memory graph, cortex admin chat, channel viewer, cron management, config editor, file ingestion UI.
**Why it matters**: Operators need visibility into what the agent is doing. CLI-only operation doesn't scale.
**Pathway**:
- [x] Decision: consolidate dashboard UI in gateway-served surfaces (no separate frontend repo) — see `docs/architecture/adr-006-dashboard-ui-stack.md` (`#2754`)
- [x] Tech stack: React + TypeScript + Vite selected for richer SPA evolution (incremental migration from embedded shell) — see `docs/architecture/adr-006-dashboard-ui-stack.md` (`#2754`)
- [x] Priority pages: Overview dashboard, Session viewer, Memory browser, Configuration editor
- [x] Stretch page: Memory graph visualization
- [x] Stretch page: Cortex admin chat
- [x] Stretch page: Cron management
- [x] Serve embedded SPA from gateway (rust-embed or include_bytes)
- Progress evidence: #2614 (gateway dashboard operator tab baseline), #2667 (PRD memory explorer API foundation: entry CRUD + filtered search), #2727 (memory graph force-layout + `/api/memories/graph` parity), #2730 (cortex admin webchat panel), #2734 (webchat routines/cron management panel + live gateway status/jobs/cancel validation), #2738 (embedded `/dashboard` SPA shell served by gateway + status discovery wiring), #2742 (API-backed overview/sessions/memory/configuration priority pages in `/dashboard`), #2746 (ADR-backed architecture/stack decision closure), #2754 (ADR-backed G18 decision/stack checklist reconciliation)
- **Files**: `crates/tau-gateway/src/gateway_openresponses/dashboard_shell.html`, `docs/architecture/adr-006-dashboard-ui-stack.md`
- **Effort**: Large

#### G19. Memory Graph Visualization
**What**: Interactive force-directed graph showing memory nodes and their relations, built with Sigma.js + Graphology.
**Why it matters**: Makes the agent's knowledge structure visible and debuggable.
**Pathway**:
- [x] Requires G6 (memory graph relations) first
- [x] Add `GET /api/memories/graph` endpoint to gateway returning nodes + edges JSON
- [x] Frontend component using Sigma.js or D3.js force layout
- [x] Node size by importance, edge color by relation type
- **Files**: Gateway routes, frontend component
- **Effort**: Medium (after G6)

#### G20. Encrypted Secrets Store
**What**: AES-256-GCM encrypted secret storage in redb, with `DecryptedSecret` wrapper that redacts in Debug/Display.
**Why it matters**: API keys stored in plain text in config files is a security concern.
**Pathway**:
- [ ] Add `aes-gcm` and `redb` as dependencies (or use existing rusqlite)
- [x] Create `SecretStore` trait in `tau-provider` or `tau-safety` (`#2652`)
- [x] Encrypt secrets at rest with machine-derived key or user-provided passphrase (`#2652`)
- [x] `DecryptedSecret` wrapper type that implements `Debug`/`Display` as `"[REDACTED]"` (`#2652`)
- [x] Migrate API key storage from plain-text TOML to encrypted store (`#2657`)
- **Files**: `tau-provider/src/` or `tau-safety/src/`
- **Effort**: Medium

#### G21. OpenCode/External Coding Agent Integration
**What**: Spacebot spawns OpenCode as a persistent subprocess for deep coding sessions, communicating via HTTP+SSE.
**Why it matters**: Enables specialized coding workflows that go beyond simple tool use.
**Pathway**:
- [x] Add external coding agent subprocess support to worker system
- [x] HTTP+SSE communication protocol for streaming progress
- [x] Server pool for managing persistent processes per workspace
- [x] Interactive follow-up support (send additional context to running session)
- [x] 10-minute inactivity timeout with auto-cleanup
- Progress evidence: #2619 (runtime bridge staging), #2638 (gateway HTTP+SSE integration), and #2647 (worker subprocess execution integration)
- **Files**: New module in `tau-tools/` or `tau-runtime/`
- **Effort**: Medium-Large

#### G22. Skills System (SKILL.md)
**What**: Markdown files with YAML frontmatter defining reusable skill templates that are injected into agent prompts.
**Why it matters**: Modular, user-authored behavior customization without code changes.
**Pathway**:
- [x] Tau already has `tau-skills` with a package manifest system — this is more sophisticated than Spacebot's SKILL.md
- [x] To add SKILL.md compatibility: parse markdown + YAML frontmatter alongside existing skill format
- [x] Inject skill summaries into channel prompts, full content into worker/delegated prompts
- [x] Support `{baseDir}` template variable
- Progress evidence: #2642
- **Files**: `tau-skills/src/`
- **Effort**: Small (Tau's existing skills system is already more advanced)

#### G23. Fly.io Deployment Config
**What**: Pre-configured `fly.toml` for one-command cloud deployment.
**Why it matters**: Reduces deployment friction for teams who don't want to manage infrastructure.
**Pathway**:
- [x] Add `fly.toml` to repo root with sensible defaults
- [x] Document in deployment guide
- [ ] Add Fly.io to CI/CD pipeline (optional step)
- **Files**: `fly.toml`, `.github/workflows/`
- **Effort**: Small

---

## Priority Matrix

| Priority | Gap | Effort | Impact |
|----------|-----|--------|--------|
| **P0** | G2. Context Compaction | Medium | Prevents context overflow in long conversations |
| **P0** | G8. Local Embeddings | Small | Eliminates API cost/latency for every memory operation |
| **P0** | G11. Message Coalescing | Small | Critical for multi-user channels |
| **P1** | G5. Typed Memories | Small-Medium | Foundation for G6, G7 |
| **P1** | G10. Discord Adapter | Medium-Large | Largest community platform |
| **P1** | G15. Process-Type Routing | Medium | Major cost optimization |
| **P1** | G4. Branch-as-Tool | Medium | Cleaner thinking/responding separation |
| **P2** | G6. Memory Graph | Medium | Requires G5 first |
| **P2** | G7. Memory Lifecycle | Medium | Requires G5 first |
| **P2** | G9. Memory Ingestion | Medium | Bulk knowledge import |
| **P2** | G16. Hot-Reload Config | Small-Medium | Production convenience |
| **P2** | G17. Template Prompts | Medium | Operator customization |
| **P2** | G12. Skip Tool | Small | Multi-user behavior |
| **P2** | G13. React Tool | Small | Multi-user behavior |
| **P2** | G14. Send File Tool | Small | Artifact delivery |
| **P3** | G1. Multi-Process Architecture | Large | Full multi-user concurrency |
| **P3** | G3. Cortex | Large | Cross-session intelligence |
| **P3** | G18. Full Web Dashboard | Large | Operator visibility |
| **P3** | G20. Encrypted Secrets | Medium | Security hardening |
| **P4** | G19. Memory Graph Viz | Medium | Debugging aid |
| **P4** | G21. External Coding Agent | Medium-Large | Specialized workflows |
| **P4** | G22. SKILL.md Compat | Small | Tau already has richer system |
| **P4** | G23. Fly.io Config | Small | Deployment convenience |

---

## The Bottom Line

**Spacebot's core advantage**: The 5-process architecture and typed memory graph. These are genuinely novel design choices that solve real problems (multi-user concurrency and knowledge structure).

**Tau's core advantages**: Scale (244K lines vs ~20K), depth (42 crates, 2,573 tests, 35-model catalog), security (4-tier tool policy, SSRF, sandbox, RBAC), training (PPO/GAE + APO), identity (KAMN DID), and operational maturity (6-platform releases, Docker, Homebrew, shell completions, live validation harness).

**Where they converge**: Both are pure Rust, both use axum for HTTP, both have hybrid memory search (vector + FTS + RRF), both support Slack, both have session persistence, both have secret leak detection.

**The biggest gaps to close**: Context compaction (G2), local embeddings (G8), and message coalescing (G11) are high-impact, low-effort. They should be first. The 5-process architecture (G1) and Cortex (G3) are the most architecturally significant but also the largest effort — they should be designed carefully, not rushed.

Spacebot is 6 days old and impressive for its scope. Tau is months of production engineering and 12x the codebase. The right strategy is to adopt Spacebot's best ideas (compaction, typed memory, process-type routing, branch-as-tool) without attempting to replicate its entire architecture — Tau already has capabilities Spacebot doesn't (training, WASM, MCP, KAMN, orchestration, session DAG).
