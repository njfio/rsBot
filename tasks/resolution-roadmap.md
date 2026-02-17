# Tau — What Needs To Be Resolved

**Snapshot Date**: 2026-02-16
**HEAD**: `ff478eec` (branch `codex/issue-916-gateway-dashboard-health`)
**Codebase**: 238,723 lines | 377 Rust files | 45 crates | ~7,805 tests
**Open Issues**: 1 (Epic M39 — rustdoc wave 12)

## Execution Update (2026-02-16)

Implemented in M42.1.1a (`#2219`):

- [x] Extended `ModelCatalogEntry` metadata fields:
  `supports_extended_thinking`, `max_output_tokens`, `knowledge_cutoff`,
  `deprecated`, `cached_input_cost_per_million`.
- [x] Refreshed built-in catalog entries to include current frontier/legacy
  coverage, including GPT-5.x, GPT-4.1, O-series, Claude 4.x, Gemini 3/2.5,
  and DeepSeek aliases; removed duplicate `openai/gpt-4o-mini` and
  sunset `gemini-2.0-flash`.
- [x] Added `deepseek` provider alias routing to OpenAI-compatible provider path.
- [x] Added DeepSeek/OpenRouter Tau-prefixed env key candidates to provider auth
  resolution (`TAU_DEEPSEEK_API_KEY`, `TAU_OPENROUTER_API_KEY`).
- [x] Added local-safe live validation harness:
  `scripts/dev/provider-keys.env.example`,
  `scripts/dev/provider-live-smoke.sh`,
  quickstart runbook updates.

Still pending in this roadmap:

- [x] First-class OpenRouter provider enum variant and dedicated client config.
- [ ] Dynamic catalog discovery and remote-source merge policy.
- [x] PPO runtime integration baseline call path.

### Critical Gap Revalidation (2026-02-17)

The following previously reported critical claims were revalidated against the
current codebase with executable tests.

| Claim | Status | Evidence |
|---|---|---|
| No per-session cost tracking | Resolved | `cargo test -p tau-session integration_session_usage_summary_persists_across_store_reload -- --nocapture` |
| No token pre-flight estimation | Resolved | `cargo test -p tau-gateway integration_spec_c01_openresponses_preflight_blocks_over_budget_request -- --nocapture` |
| No prompt caching | Resolved | `cargo test -p tau-ai spec_c01_openai_serializes_prompt_cache_key_when_enabled -- --nocapture`; `cargo test -p tau-ai spec_c02_anthropic_serializes_system_cache_control_when_enabled -- --nocapture`; `cargo test -p tau-ai spec_c03_google_serializes_cached_content_reference_when_enabled -- --nocapture` |
| PPO/GAE never called from training loop | Resolved | `cargo test -p tau-coding-agent spec_c02_integration_prompt_optimization_mode_executes_rl_optimizer_when_enabled -- --nocapture`; call site `crates/tau-coding-agent/src/training_runtime.rs` |

### Gap Revalidation Wave 2 (2026-02-17)

Functional/distribution/ops claims (5-15) were revalidated with
`scripts/dev/verify-gap-claims-wave2.sh`.

| Claim | Status | Evidence |
|---|---|---|
| OpenRouter still alias, not first-class provider | Resolved | `cargo test -p tau-ai spec_c01_parses_openrouter_as_first_class_provider -- --nocapture`; `cargo test -p tau-ai spec_c06_openrouter_route_applies_dedicated_headers_when_configured -- --nocapture` |
| PostgreSQL session backend scaffolded/not implemented | Resolved | `cargo test -p tau-session spec_c05_postgres_invalid_dsn_reports_backend_error_not_scaffold -- --nocapture`; `scripts/dev/verify-session-postgres-live.sh` (boots ephemeral docker postgres and runs `integration_spec_c02..c04`) |
| Onboarding wizard partial (state detection only) | Resolved | `cargo test -p tau-onboarding functional_spec_c01_c02_execute_onboarding_command_guided_flow_is_deterministic_and_applies_selected_workspace -- --nocapture` |
| Dashboard scaffold only | Resolved | `scripts/dev/verify-dashboard-consolidation.sh` (validates gateway dashboard endpoints/actions/stream/auth regressions and onboarding rejection of removed dashboard contract runner); ADR: `docs/architecture/adr-001-dashboard-consolidation.md` |
| WASI preview 1, not preview 2 | Resolved | `cargo test -p tau-deployment spec_c03_wasi_preview2_compliance_rejects_preview1_import_modules -- --nocapture` |
| No Docker image | Resolved | `test -f Dockerfile`; `scripts/release/test-release-workflow-contract.sh` |
| No Homebrew formula | Resolved | `scripts/release/test-release-workflow-contract.sh` (checks Homebrew render + `dist/tau.rb` release asset wiring) |
| No shell completions | Resolved | `scripts/release/test-release-workflow-contract.sh` (checks generation and release assets for bash/zsh/fish) |
| No systemd unit | Resolved | `cargo test -p tau-ops spec_c01_render_systemd_user_unit_includes_required_sections_and_gateway_flags -- --nocapture` |
| No fuzz testing | Resolved | `TAU_CARGO_FUZZ_RUNS=200 scripts/dev/verify-cargo-fuzz-baseline.sh`; `cargo test -p tau-runtime spec_c01_rpc_raw_fuzz_conformance_no_panic_for_10000_inputs -- --nocapture`; `cargo test -p tau-runtime spec_c02_rpc_ndjson_fuzz_conformance_no_panic_for_10000_inputs -- --nocapture`; `cargo test -p tau-gateway spec_c03_gateway_ws_parse_fuzz_conformance_no_panic_for_10000_inputs -- --nocapture` |
| No log rotation | Resolved | `cargo test -p tau-runtime spec_c04_tool_audit_logger_rotates_and_keeps_writing_after_threshold -- --nocapture`; `cargo test -p tau-gateway spec_c04_gateway_cycle_report_rotates_and_keeps_latest_record -- --nocapture` |

---

## 1. Model Catalog — Update to Latest Models

### Current State (Stale)

10 built-in models in `crates/tau-provider/src/model_catalog.rs` — many are outdated or retired:

| # | Provider | Model | Context | Input $/M | Output $/M | Status |
|---|----------|-------|---------|-----------|------------|--------|
| 1 | OpenAI | gpt-4o-mini | 128K | $0.15 | $0.60 | **RETIRED from ChatGPT** (Feb 13, 2026), still in API |
| 2 | OpenAI | gpt-4o | 128K | $2.50 | $10.00 | **RETIRED from ChatGPT** (Feb 13, 2026), still in API |
| 3 | OpenAI | openai/gpt-4o-mini | 128K | $0.15 | $0.60 | **DUPLICATE of #1** — remove |
| 4 | OpenAI | llama-3.3-70b | 128K | — | — | Superseded by Llama 4 family |
| 5 | OpenAI | grok-4 | 128K | — | — | Missing pricing, wrong context (should be 256K) |
| 6 | OpenAI | mistral-large-latest | 128K | — | — | Superseded by Mistral Large 3 |
| 7 | Anthropic | claude-sonnet-4 | 200K | $3.00 | $15.00 | **LEGACY** — superseded by Sonnet 4.5 |
| 8 | Anthropic | claude-3-5-haiku-latest | 200K | $0.80 | $4.00 | **LEGACY** — superseded by Haiku 4.5 |
| 9 | Google | gemini-2.0-flash | 1M | $0.10 | $0.40 | **DEPRECATED** — shutting down March 31, 2026 |
| 10 | Google | gemini-2.5-pro | 1M | $1.25 | $10.00 | Current but no longer flagship |

**Verdict**: 8 of 10 entries are outdated, retired, deprecated, or duplicated. The catalog needs a complete overhaul.

### Target Catalog — Current Frontier Models (February 2026)

#### OpenAI — GPT-5.x Generation (Flagship)

| Model ID | Context | Tools | Multimodal | Thinking | Input $/M | Output $/M | Notes |
|----------|---------|-------|------------|----------|-----------|------------|-------|
| `gpt-5.2` | 400K | yes | yes | yes (xhigh) | $1.75 | $14.00 | Current flagship, SOTA on GDPval |
| `gpt-5.2-pro` | 400K | yes | yes | yes | $21.00 | $168.00 | Maximum capability tier |
| `gpt-5` | 400K | yes | yes | yes | $1.25 | $10.00 | Previous gen, still capable |
| `gpt-5-codex` | 400K | yes | yes | yes | $1.25 | $10.00 | Coding-optimized GPT-5 |

#### OpenAI — GPT-4.1 Generation (Value Tier, 1M Context)

| Model ID | Context | Tools | Multimodal | Thinking | Input $/M | Output $/M | Notes |
|----------|---------|-------|------------|----------|-----------|------------|-------|
| `gpt-4.1` | 1M | yes | yes | yes | $2.00 | $8.00 | Best value for long context |
| `gpt-4.1-mini` | 1M | yes | yes | yes | $0.40 | $1.60 | Cost-efficient workhorse |
| `gpt-4.1-nano` | 1M | yes | yes | yes | $0.10 | $0.40 | Fastest/cheapest OpenAI |

#### OpenAI — O-Series (Reasoning-First)

| Model ID | Context | Tools | Multimodal | Thinking | Input $/M | Output $/M | Notes |
|----------|---------|-------|------------|----------|-----------|------------|-------|
| `o3` | 200K | yes | no | yes | $2.00 | $8.00 | Deep reasoning |
| `o4-mini` | 200K | yes | yes | yes | $1.10 | $4.40 | Budget reasoning + multimodal |

#### OpenAI — Legacy (Keep for Backward Compat)

| Model ID | Context | Tools | Multimodal | Thinking | Input $/M | Output $/M | Notes |
|----------|---------|-------|------------|----------|-----------|------------|-------|
| `gpt-4o` | 128K | yes | yes | yes | $2.50 | $10.00 | Retired from ChatGPT, API only |
| `gpt-4o-mini` | 128K | yes | yes | yes | $0.15 | $0.60 | Retired from ChatGPT, API only |

#### Anthropic — Claude 4.x Generation

| Model ID | Context | Tools | Multimodal | Thinking | Input $/M | Output $/M | Notes |
|----------|---------|-------|------------|----------|-----------|------------|-------|
| `claude-opus-4-6` | 200K (1M beta) | yes | yes | yes (adaptive) | $5.00 | $25.00 | **Current flagship** (Feb 5, 2026), 128K output |
| `claude-sonnet-4-5` | 200K (1M beta) | yes | yes | yes | $3.00 | $15.00 | Best speed/intelligence balance, 64K output |
| `claude-haiku-4-5` | 200K | yes | yes | yes | $1.00 | $5.00 | Fastest, near-frontier, 64K output |

#### Anthropic — Legacy (Keep for Backward Compat)

| Model ID | Context | Tools | Multimodal | Thinking | Input $/M | Output $/M | Notes |
|----------|---------|-------|------------|----------|-----------|------------|-------|
| `claude-sonnet-4-0` | 200K (1M beta) | yes | yes | yes | $3.00 | $15.00 | Previous Sonnet |
| `claude-opus-4-5` | 200K | yes | yes | yes | $5.00 | $25.00 | Previous Opus |

#### Google — Gemini 3 Generation (Preview)

| Model ID | Context | Tools | Multimodal | Thinking | Input $/M | Output $/M | Notes |
|----------|---------|-------|------------|----------|-----------|------------|-------|
| `gemini-3-pro-preview` | 1M | yes | yes | yes (levels) | $2.00 | $12.00 | New flagship, computer use support |
| `gemini-3-flash-preview` | 1M | yes | yes | yes | $0.50 | $3.00 | Pro-level intelligence at Flash price |

#### Google — Gemini 2.5 Generation (Stable)

| Model ID | Context | Tools | Multimodal | Thinking | Input $/M | Output $/M | Notes |
|----------|---------|-------|------------|----------|-----------|------------|-------|
| `gemini-2.5-pro` | 1M | yes | yes | yes | $1.25 | $10.00 | Stable, production-ready |
| `gemini-2.5-flash` | 1M | yes | yes | yes | $0.30 | $2.50 | Best price/performance |
| `gemini-2.5-flash-lite` | 1M | yes | yes | no | $0.10 | $0.40 | Cheapest Gemini |

#### xAI — Grok

| Model ID | Context | Tools | Multimodal | Thinking | Input $/M | Output $/M | Notes |
|----------|---------|-------|------------|----------|-----------|------------|-------|
| `grok-4` | 256K | yes | yes | yes | $3.00 | $15.00 | Flagship |
| `grok-4-fast` | 2M | yes | yes | yes | $0.20 | $0.50 | Speed-optimized, largest context |
| `grok-code-fast-1` | 256K | yes | no | yes | $0.20 | $1.50 | Coding-specialized |

#### Mistral AI

| Model ID | Context | Tools | Multimodal | Thinking | Input $/M | Output $/M | Notes |
|----------|---------|-------|------------|----------|-----------|------------|-------|
| `mistral-large-3` | 256K | yes | yes | yes | $0.50 | $1.50 | Apache 2.0, 675B MoE |
| `mistral-medium-3` | 131K | yes | yes | yes | $0.40 | $2.00 | Mid-tier |
| `mistral-small-3.1-24b` | 131K | yes | no | no | $0.03 | $0.11 | Budget open-weight |

#### Meta — Llama 4 (via OpenAI-compatible providers)

| Model ID | Context | Tools | Multimodal | Thinking | Input $/M | Output $/M | Notes |
|----------|---------|-------|------------|----------|-----------|------------|-------|
| `llama-4-scout` | 10M | yes | yes | no | $0.15 | $0.50 | 109B MoE, open weights |
| `llama-4-maverick` | 1M | yes | yes | no | $0.22 | $0.85 | 400B MoE, open weights |

#### DeepSeek

| Model ID | Context | Tools | Multimodal | Thinking | Input $/M | Output $/M | Notes |
|----------|---------|-------|------------|----------|-----------|------------|-------|
| `deepseek-chat` (V3.2) | 164K | yes | no | no | $0.28 | $0.42 | Cache hit: $0.028 |
| `deepseek-reasoner` (V3.2) | 164K | yes | no | yes | $0.28 | $0.42 | Reasoning mode of V3.2 |

### Implementation Checklist

- [ ] **Remove deprecated entries**: delete `gemini-2.0-flash` (shutting down March 31), remove duplicate `openai/gpt-4o-mini`
- [ ] **Update existing entries**: `claude-sonnet-4` → `claude-sonnet-4-5`, `claude-3-5-haiku-latest` → `claude-haiku-4-5`, `mistral-large-latest` → `mistral-large-3`
- [ ] **Add GPT-5.x family**: `gpt-5.2`, `gpt-5`, `gpt-5-codex` (gpt-5.2-pro and gpt-5.3-codex may not have API access yet)
- [ ] **Add GPT-4.1 family**: `gpt-4.1`, `gpt-4.1-mini`, `gpt-4.1-nano`
- [ ] **Add O-series**: `o3`, `o4-mini`
- [ ] **Add Claude 4.6**: `claude-opus-4-6` as new flagship
- [ ] **Add Gemini 3**: `gemini-3-pro-preview`, `gemini-3-flash-preview`
- [ ] **Add Gemini 2.5 Flash**: `gemini-2.5-flash`, `gemini-2.5-flash-lite`
- [ ] **Add Grok models**: `grok-4` (fix pricing + context), `grok-4-fast`, `grok-code-fast-1`
- [ ] **Add Mistral 3**: `mistral-large-3`, `mistral-medium-3`
- [ ] **Add Llama 4**: `llama-4-scout`, `llama-4-maverick`
- [ ] **Add DeepSeek**: `deepseek-chat`, `deepseek-reasoner`
- [ ] **Move `gpt-4o`, `gpt-4o-mini` to legacy section** with deprecation notice

### Catalog Schema Changes

- [ ] **Add `supports_extended_thinking` field** to `ModelCatalogEntry` — needed for all models with thinking/reasoning traces (GPT-5.2, o3, o4-mini, Claude Opus 4.6, Gemini 3, etc.)
- [ ] **Add `max_output_tokens` field** — varies wildly: 4K (Haiku 3) → 64K (Sonnet 4.5) → 128K (Opus 4.6)
- [ ] **Add `knowledge_cutoff` field** — important for users choosing models for current-events tasks
- [ ] **Add `deprecated` field** — boolean flag for retired/sunset models with optional sunset date
- [ ] **Add `cached_input_cost_per_million` field** — cached input pricing is a major cost differentiator (GPT-5.x: 90% off, GPT-4.1: 75% off)
- [ ] **Remote catalog auto-refresh** — infrastructure exists (`model_catalog_url`, `model_catalog_cache`) but needs a default public endpoint or OpenRouter's `/api/v1/models` as source

---

## 2. OpenRouter — First-Class Provider Support

### Current State

OpenRouter is an alias that routes through the OpenAI provider:
- `crates/tau-ai/src/provider.rs:44` — `"openrouter" => Provider::OpenAi` with model prefix `openai/{model}`
- Uses the same OpenAI HTTP client, API base, and auth
- OpenRouter now provides 500+ models from 60+ providers

### What's Needed

#### 2.1 Provider Enum Variant
- [ ] Add `OpenRouter` as a fourth variant in `Provider` enum (`crates/tau-ai/src/provider.rs`)
- [ ] Update `as_str()`, `FromStr`, and `Display` implementations
- [ ] Update all match arms across the codebase that pattern-match on `Provider`

#### 2.2 Dedicated Client Configuration
- [ ] Add `OpenRouterConfig` struct in `tau-provider` with:
  - `api_base`: default `https://openrouter.ai/api/v1`
  - `api_key`: from `TAU_OPENROUTER_API_KEY` env var
  - `site_url`: optional `HTTP-Referer` header (OpenRouter ranking)
  - `site_name`: optional `X-Title` header (OpenRouter dashboard identification)
- [ ] Add `OpenRouterClient` that wraps OpenAI-compatible transport but includes OpenRouter-specific headers

#### 2.3 CLI Arguments
- [ ] Add `--openrouter-api-key` / `TAU_OPENROUTER_API_KEY` CLI arg in `cli_args.rs`
- [ ] Add `--openrouter-api-base` with default `https://openrouter.ai/api/v1`
- [ ] Add `--openrouter-site-url` and `--openrouter-site-name` optional args
- [ ] Update help text for `--model` to list `openrouter` as a first-class provider

#### 2.4 Auth Flow
- [ ] Add OpenRouter auth detection in `tau-provider/src/auth.rs`
- [ ] Support API key authentication (primary)
- [ ] Support OAuth via OpenRouter's PKCE flow (stretch)

#### 2.5 Model Catalog Entries
- [ ] Add OpenRouter-specific entries for popular models:
  - `openrouter/auto` — OpenRouter's auto-routing
  - `openrouter/openai/gpt-5.2` — OpenAI via OpenRouter
  - `openrouter/anthropic/claude-opus-4-6` — Anthropic via OpenRouter
  - `openrouter/google/gemini-3-pro-preview` — Google via OpenRouter
  - `openrouter/meta-llama/llama-4-maverick` — Meta via OpenRouter
  - `openrouter/deepseek/deepseek-chat` — DeepSeek via OpenRouter
  - `openrouter/mistralai/mistral-large-3` — Mistral via OpenRouter
- [ ] Populate cost data from OpenRouter's pricing API

#### 2.6 Dynamic Model Discovery
- [ ] Fetch available models from OpenRouter's `GET /api/v1/models` endpoint
- [ ] Parse standardized JSON response with model metadata (pricing, context, capabilities)
- [ ] Cache locally and merge with built-in catalog
- [ ] This replaces the need to manually add every OpenRouter model — 500+ models auto-discovered

#### 2.7 Fallback Integration
- [ ] Wire `OpenRouterClient` into `FallbackRoutingClient` in `tau-provider/src/fallback.rs`
- [ ] Support OpenRouter as a universal fallback (e.g., primary Anthropic API → fallback OpenRouter/Anthropic)
- [ ] Circuit breaker support for OpenRouter routes

#### 2.8 OpenRouter-Specific Features (Stretch)
- [ ] Model routing preferences (`route: "fallback"` parameter)
- [ ] Cost tracking via OpenRouter's generation metadata
- [ ] Provider preferences (`provider.order`, `provider.allow`, `provider.deny`)
- [ ] Zero Completion Insurance — auto-retry with alternative models on failure

---

## 3. DeepSeek — Add as Provider Alias

### Rationale
DeepSeek V3.2 is the most cost-effective model available ($0.28/$0.42 per M tokens, with $0.028 cache hits). It uses OpenAI-compatible API format, making integration trivial.

### What's Needed
- [ ] Add `"deepseek"` alias in `Provider::from_str()` → routes to `Provider::OpenAi`
- [ ] Default API base: `https://api.deepseek.com`
- [ ] Add `--deepseek-api-key` / `TAU_DEEPSEEK_API_KEY` env var
- [ ] Add catalog entries for `deepseek-chat` and `deepseek-reasoner`
- [ ] Document cache-hit pricing advantage in catalog metadata

---

## 4. PPO/RL Training Loop Integration

### Current State

PPO and GAE are implemented as pure math modules but are **never called from the training pipeline**:

- `crates/tau-algorithm/src/ppo.rs` — `compute_ppo_loss()`, `compute_ppo_update()` — fully tested, 12 test cases
- `crates/tau-algorithm/src/gae.rs` — `compute_gae_batch_from_trajectory()` — fully tested, 5 test cases
- **Zero production callers** — only referenced from tests within `tau-algorithm`

The actual training loop uses APO (Automatic Prompt Optimization) — beam search over prompts via LLM critique/edit/score cycles. The loop closes via resource updates (system prompt replacement), not gradient descent.

### What's Needed

#### 4.1 Reward Signal Pipeline
- [ ] Define `RewardSignal` trait in `tau-algorithm` with methods for computing scalar rewards from agent trajectories
- [ ] Implement concrete reward signals: task completion, tool success rate, safety compliance, user feedback
- [ ] Wire `TauAgentExecutor` (in `tau-training-runner`) to emit structured reward signals alongside existing `exact_match` scoring

#### 4.2 Trajectory → PPO Sample Conversion
- [ ] Create `TrajectoryToPpoConverter` that maps `TrainingSpan` sequences into `PpoSample` structs
- [ ] Map span duration, tool success/failure, and evaluator scores to advantages
- [ ] Compute value predictions from a baseline (average reward, exponential moving average, or learned critic)

#### 4.3 GAE Integration into Training Loop
- [ ] Call `compute_gae_batch_from_trajectory()` on each completed training episode
- [ ] Feed GAE advantages into PPO loss computation
- [ ] Make gamma (0.99) and lambda (0.95) configurable via training config TOML

#### 4.4 PPO Update Step
- [ ] Implement `PpoTrainer` that orchestrates: collect trajectories → compute GAE → compute PPO loss → apply update
- [ ] For LLM agents, "applying the update" means: adjusting prompt templates, tool selection weights, or fine-tuning parameters via API
- [ ] Support multiple PPO epochs per batch (configurable)
- [ ] Implement KL penalty coefficient auto-tuning (target KL divergence)

#### 4.5 Integration with APO
- [ ] Option A: PPO as an alternative to APO (user selects training algorithm)
- [ ] Option B: PPO layered on top of APO (PPO selects which APO-generated prompt variants to keep)
- [ ] Option C: Hybrid — APO for prompt structure, PPO for behavioral policy (tool selection, response style)
- [ ] Add `--training-algorithm` CLI flag: `apo` (default), `ppo`, `hybrid`

#### 4.6 Agent Lightning Patterns to Adopt
- [ ] Hierarchical credit assignment (Microsoft Agent Lightning's key innovation) — attribute rewards to individual tool calls within multi-step trajectories
- [ ] GRPO (Group Relative Policy Optimization) as an alternative to PPO for LLM agents — no critic network needed
- [ ] Trajectory tree structure for branching agent executions

---

## 5. Error Handling — .expect() Audit

### Current State

~16,800 `.expect()` calls across the workspace. While many are in tests (acceptable), production code has significant `.expect()` density that should be converted to proper error propagation.

### Priority Files for Conversion

| File | Lines | Priority | Notes |
|------|-------|----------|-------|
| `tau-cli/src/cli_args.rs` | 3,723 | HIGH | CLI parsing — panics are user-visible |
| `tau-agent-core/src/lib.rs` | 2,476 | HIGH | Core runtime — panics crash the agent |
| `tau-gateway/src/gateway_openresponses.rs` | 2,359 | HIGH | Gateway — panics crash the server |
| `tau-training-runner/src/lib.rs` | 2,323 | MEDIUM | Training — panics lose training progress |
| `tau-runtime/src/rpc_protocol_runtime.rs` | 2,274 | MEDIUM | RPC — panics disconnect clients |
| `tau-release-channel/src/command_runtime.rs` | 2,341 | MEDIUM | Commands — panics lose work |
| `tau-github-issues-runtime/src/github_issues_runtime.rs` | 2,620 | LOW | GitHub runtime |
| `tau-skills/src/package_manifest.rs` | 3,047 | LOW | Manifest parsing |

### Replacement Patterns
- [ ] **Lock acquisition**: `.lock().expect("...")` → `.lock().map_err(|_| anyhow!("poisoned lock"))?` or recover with `into_inner()`
- [ ] **Channel operations**: `.send().expect(...)` → `.send().map_err(|e| ...)?`
- [ ] **Serialization**: `serde_json::to_string().expect(...)` → `serde_json::to_string()?`
- [ ] **Configuration access**: `.get("key").expect(...)` → `.get("key").ok_or_else(|| anyhow!("missing key"))?`
- [ ] **Index access**: `vec[i].expect(...)` → `vec.get(i).ok_or_else(|| ...)?`

### Target
- [ ] Reduce production `.expect()` calls by 80% (keep only truly unreachable cases with `// SAFETY:` comments)
- [ ] Zero `.expect()` in gateway/server paths (panics kill all connections)
- [ ] Zero `.expect()` in CLI argument parsing (panics show ugly backtraces to users)

---

## 6. Large File Refactoring

### Production Files Over 2,000 Lines

| File | Lines | Action |
|------|-------|--------|
| `tau-cli/src/cli_args.rs` | 3,723 | Split into: `cli_args/model.rs`, `cli_args/auth.rs`, `cli_args/runtime.rs`, `cli_args/training.rs`, `cli_args/gateway.rs` |
| `tau-skills/src/package_manifest.rs` | 3,047 | Split into: `package_manifest/parser.rs`, `package_manifest/validation.rs`, `package_manifest/resolution.rs` |
| `tau-github-issues-runtime/src/github_issues_runtime.rs` | 2,620 | Split into: `github_issues/api.rs`, `github_issues/sync.rs`, `github_issues/rendering.rs` |
| `tau-agent-core/src/lib.rs` | 2,476 | Continue splitting into lifecycle modules (started: 4 modules extracted, more remain) |
| `tau-gateway/src/gateway_openresponses.rs` | 2,359 | Split into: `openresponses/mapping.rs`, `openresponses/streaming.rs`, `openresponses/validation.rs` |
| `tau-release-channel/src/command_runtime.rs` | 2,341 | Split by command type into submodules |
| `tau-training-runner/src/lib.rs` | 2,323 | Split into: `executor.rs`, `reward.rs`, `rollout.rs`, `safety_policy.rs` |
| `tau-runtime/src/rpc_protocol_runtime.rs` | 2,274 | Split into: `rpc_protocol/handlers.rs`, `rpc_protocol/transport.rs`, `rpc_protocol/dispatch.rs` |

### Target
- [ ] No production `.rs` file exceeds 2,000 lines
- [ ] Each module has a single clear responsibility
- [ ] Public API surface preserved (re-export from parent module)

---

## 7. Documentation — Close the Gap

### Current State

- **Doc comments (`///`)**: ~7,805 lines
- **Target**: 9,000 lines (1 per 20 lines of code)
- **Gap**: ~1,195 lines (13% below target)
- **Active work**: Epic M39 (Wave 12) — 1 open issue remaining

### What's Needed

- [ ] Complete Epic M39 (GitHub issues runtime split modules)
- [ ] **Wave 13**: tau-cli/src/cli_args.rs — largest file, needs comprehensive arg docs
- [ ] **Wave 14**: tau-training-runner, tau-trainer, tau-algorithm — training subsystem
- [ ] **Wave 15**: tau-gateway/src/gateway_openresponses.rs — OpenAI-compatible API surface
- [ ] **Wave 16**: tau-skills/src/package_manifest.rs — package system
- [ ] **Executable doctests**: Add `/// # Examples` blocks to top-20 most-used public functions
- [ ] **Module-level docs**: Every `mod.rs` / `lib.rs` should have `//!` module documentation
- [ ] **Architecture docs**: Update `docs/` with current system architecture (session DAG, multi-channel pipeline, training loop)

---

## 8. Scaffold Crates — Finish or Remove

### Status by Crate

| Crate | Lines | Real I/O | Status | Action |
|-------|-------|----------|--------|--------|
| tau-browser-automation | ~800 | subprocess via `std::process::Command` | Hybrid: contract fixture + live execution adapter | **Finish**: implement real CDP/Playwright integration or document subprocess-only scope |
| tau-custom-command | ~600 | `tokio::process::Command` | Hybrid: contract fixture + real subprocess | **Finish**: remove contract fixture layer, wire directly as event handler |
| tau-dashboard | ~1,900 | Limited (fixture replay) | Mostly fixture-based | **Decide**: build real web dashboard frontend or scope as status API only |
| tau-memory | ~500 | `rusqlite` (SQLite backend) | **Real** — has SQLite persistence | **Done**: graduated from scaffold to real |
| tau-voice | varies | Conditional on `runtime` feature | Feature-gated | **Finish**: ensure runtime feature works end-to-end, add integration test |

### Action Items
- [ ] **tau-browser-automation**: Either integrate headless Chrome/Playwright via CDP protocol or document current subprocess approach as intentional scope
- [ ] **tau-custom-command**: Strip contract fixture indirection — the `tokio::process::Command` execution is real, the fixture wrapper adds unnecessary complexity
- [ ] **tau-dashboard**: Decision needed — if web UI exists in tau-gateway already, remove dashboard contract infrastructure and consolidate there
- [ ] **tau-voice**: Enable runtime feature by default or add feature-gated integration test

---

## 9. Security Hardening

### Completed
- [x] tau-safety crate with Aho-Corasick pattern scanner + regex leak detection
- [x] Wired into agent-core (sanitizer + leak detector as pluggable `Arc<dyn>`)
- [x] SSRF protection (RFC 1918, loopback, link-local, cloud metadata blocking)
- [x] Fail-closed sandbox policy
- [x] Identity file protection (protected paths)
- [x] Protected tool name registry

### Remaining
- [ ] **Rate limiting enforcement** — `ToolPolicy` has rate limit fields but verify they're enforced in the hot path, not just configured
- [ ] **Audit log** — security-relevant events (blocked injections, redacted secrets, SSRF blocks) should write to a dedicated audit log, not just tracing
- [ ] **Safety policy profiles** — expose named security profiles (Permissive/Balanced/Strict/Hardened) in onboarding wizard and CLI
- [ ] **Fuzz testing** — add `cargo fuzz` targets for prompt injection patterns and secret detection regex
- [ ] **Dependency audit automation** — add `cargo audit` to CI (check if present in ci.yml)
- [ ] **Content Security Policy** for gateway web UI — prevent XSS in webchat

---

## 10. Testing Improvements

### Current State
- ~7,805 `#[test]` functions
- Strong unit test coverage
- Integration tests exist for CLI, MCP, session runtime

### Gaps
- [ ] **Property-based testing** — add `proptest` or `quickcheck` for serialization roundtrips (session DAG, memory entries, training store)
- [ ] **Benchmark suite** — add `criterion` benchmarks for hot paths: memory search, session branch/merge, tool policy evaluation
- [ ] **Load testing** — gateway under concurrent connections (verify no panics under load)
- [ ] **Chaos testing** — simulate provider failures, network timeouts, disk full conditions
- [ ] **Coverage tracking** — add `cargo tarpaulin` or `cargo llvm-cov` to CI with minimum threshold
- [ ] **Snapshot testing** — for CLI output, gateway API responses, and error messages

---

## 11. Provider System Improvements

### Current Architecture
3 core providers (`OpenAi`, `Anthropic`, `Google`) with 7 aliases routing through them.

### Needed Changes
- [ ] **DeepSeek** — add as alias through OpenAI protocol (see Section 3)
- [ ] **xAI/Grok** — verify alias works correctly, update context window to 256K, add `grok-4-fast` (2M context)
- [ ] **Mistral** — update alias to route to `mistral-large-3`, add `mistral-medium-3`
- [ ] **AWS Bedrock** — add as provider variant with SigV4 auth (stretch) — provides Claude, Llama, Mistral
- [ ] **Provider health check** — `GET /health` or model list call on startup to verify credentials before accepting user input
- [ ] **Cost tracking** — accumulate token usage and cost per session, expose via `CostTool` or session metadata
- [ ] **Token counting** — pre-flight token estimation before sending to provider (prevent 413 errors)
- [ ] **Streaming improvements** — verify all providers handle SSE streaming correctly with tool calls mid-stream
- [ ] **Prompt caching support** — expose provider-specific caching (OpenAI cached inputs, Anthropic prompt caching, Google context caching) via unified API

---

## 12. Operations & Runtime

### Completed
- [x] Heartbeat system with configurable interval
- [x] Self-repair (stuck jobs, orphaned temps)
- [x] Onboarding wizard (bootstrap command)
- [x] Routine/scheduling engine with cron, events, webhooks
- [x] Job management system

### Remaining
- [ ] **Graceful shutdown** — verify all providers drain in-flight requests on SIGTERM/SIGINT
- [ ] **Session migration** — tool to export/import sessions between Tau instances
- [ ] **Multi-instance coordination** — if two Tau instances share a workspace, prevent session corruption (file locking or advisory locks)
- [ ] **Resource usage monitoring** — expose memory usage, open file handles, active connections as metrics
- [ ] **Log rotation** — ensure JSONL session files and training logs don't grow unbounded

---

## 13. Deployment & Distribution

### Completed
- [x] Multi-platform release workflow (Linux x64/ARM64, macOS x64/ARM64, Windows x64/ARM64)
- [x] SHA256 checksums and attestation
- [x] Smoke testing gates
- [x] OpenAI-compatible API endpoint in gateway

### Remaining
- [ ] **Container image** — publish official Docker image to GHCR or Docker Hub
- [ ] **Homebrew formula** — `brew install tau` for macOS users
- [ ] **Shell completion** — generate and distribute bash/zsh/fish completions via clap
- [ ] **Systemd unit file** — for running Tau gateway as a daemon on Linux
- [ ] **Helm chart** — for Kubernetes deployment of gateway + workers (stretch)
- [ ] **Version update check** — notify users when a newer release is available

---

## 14. KAMN Integration — Polish

### Completed
- [x] DID-based agent identity (kamn-core, kamn-sdk)
- [x] Reputation-gated routing
- [x] Economic coordination (escrow, cost tracking, agent-to-agent payment)
- [x] Signed envelope protocol
- [x] WASM deployment support

### Remaining
- [ ] **DID resolution** — implement DID document resolution for remote agents (not just local)
- [ ] **Key rotation** — support rotating DID verification methods without identity change
- [ ] **Revocation** — support revoking compromised agent identities
- [ ] **Cross-instance trust** — establish trust between agents running on different Tau instances
- [ ] **Integration tests** — end-to-end test with two agents performing DID-authenticated message exchange

---

## 15. WASM Runtime — Hardening

### Completed
- [x] wasmtime integration with fuel metering
- [x] Memory limits per module
- [x] Capability-based permissions

### Remaining
- [ ] **WASI preview 2** — upgrade from WASI preview 1 to preview 2 for better filesystem and networking support
- [ ] **Module caching** — cache compiled WASM modules to avoid recompilation on restart
- [ ] **Hot reload** — support updating WASM modules without restarting the agent
- [ ] **Debug support** — DWARF debugging info for WASM modules (development mode)
- [ ] **Performance profiling** — fuel consumption reports per module

---

## Priority Order

| Priority | Section | Rationale |
|----------|---------|-----------|
| **P0** | 1. Model Catalog Update | 8/10 entries outdated — GPT-5.2, Claude Opus 4.6, Gemini 3 are current |
| **P0** | 2. OpenRouter Provider | Unlocks 500+ models via single API key |
| **P0** | 3. DeepSeek Provider | Cheapest quality model available ($0.28/$0.42), OpenAI-compatible |
| **P1** | 5. .expect() Audit | Production panics are unacceptable in server mode |
| **P1** | 6. Large File Refactoring | Maintainability — 3,700-line files are unsustainable |
| **P2** | 4. PPO/RL Integration | Differentiator but requires design decision on approach |
| **P2** | 7. Documentation | 87% of target — close but needs final push |
| **P2** | 8. Scaffold Cleanup | tau-dashboard decision blocks or unblocks web UI work |
| **P3** | 9. Security Hardening | Solid foundation exists, remaining items are polish |
| **P3** | 10. Testing | Good coverage, improvements are incremental |
| **P3** | 11. Provider System | Aliases work, prompt caching is the highest-value addition |
| **P4** | 12. Operations | Working, improvements are operational maturity |
| **P4** | 13. Deployment | Release pipeline exists, packaging is distribution reach |
| **P4** | 14. KAMN Polish | Core integration done, remaining is hardening |
| **P4** | 15. WASM Hardening | Working, improvements are edge cases |

---

## Quick Wins (Can Ship This Week)

1. **Update model catalog entries** — straightforward data changes in `model_catalog.rs`:
   - Add `gpt-5.2`, `gpt-4.1`, `gpt-4.1-mini`, `gpt-4.1-nano`, `o3`, `o4-mini`
   - Add `claude-opus-4-6`, `claude-sonnet-4-5`, `claude-haiku-4-5`
   - Add `gemini-3-pro-preview`, `gemini-3-flash-preview`, `gemini-2.5-flash`, `gemini-2.5-flash-lite`
   - Fix `grok-4` pricing ($3.00/$15.00) and context (256K)
   - Add `mistral-large-3` ($0.50/$1.50)
   - Remove `gemini-2.0-flash` (deprecated), remove duplicate `openai/gpt-4o-mini`
2. **Add DeepSeek alias** — 5-line change in `provider.rs` + env var in `cli_args.rs`
3. **Fill missing cost data** for all models with `None` pricing
4. **Complete Epic M39** — 1 open issue remaining
5. **Add shell completions** — `clap` supports this with a derive macro flag
6. **Add `supports_extended_thinking` and `max_output_tokens` fields** to `ModelCatalogEntry`

---

## Research Sources

- [OpenAI Model Release Notes](https://help.openai.com/en/articles/9624314-model-release-notes)
- [OpenAI API Pricing](https://pricepertoken.com/pricing-page/provider/openai)
- [Anthropic Models Overview](https://platform.claude.com/docs/en/about-claude/models/overview)
- [Anthropic Claude Opus 4.6 Announcement](https://www.anthropic.com/news/claude-opus-4-5)
- [Google Gemini Models](https://ai.google.dev/gemini-api/docs/models)
- [Google Gemini 3 Developer Guide](https://ai.google.dev/gemini-api/docs/gemini-3)
- [Gemini API Pricing](https://ai.google.dev/gemini-api/docs/pricing)
- [xAI Models and Pricing](https://docs.x.ai/developers/models)
- [Meta Llama 4](https://www.llama.com/models/llama-4/)
- [DeepSeek API Pricing](https://api-docs.deepseek.com/quick_start/pricing)
- [Mistral AI Pricing](https://mistral.ai/pricing)
- [OpenRouter Models](https://openrouter.ai/models)
- [OpenRouter Pricing](https://openrouter.ai/pricing)
