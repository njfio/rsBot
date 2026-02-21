# Tau — What's Missing (Current State)

**Snapshot Date**: 2026-02-21  
**HEAD**: `9c6ed4fa`

This report tracks **remaining** gaps only and explicitly records what is already delivered so roadmap work stays aligned with reality.

## Resolved Since Prior Report

| Area | Current state | Evidence |
|---|---|---|
| Session cost visibility | Per-session usage and cost tracking is implemented | `crates/tau-session/src/lib.rs`, `crates/tau-agent-core/src/runtime_turn_loop.rs`, `crates/tau-gateway/src/gateway_openresponses/session_runtime.rs` |
| Token preflight | Gateway preflight enforces fail-closed token budget limits before provider dispatch | `crates/tau-gateway/src/gateway_openresponses.rs:1810`, `crates/tau-gateway/src/gateway_openresponses/tests.rs:9245` |
| Prompt caching | Prompt caching controls/usage accounting are implemented across providers | `crates/tau-ai/src/anthropic.rs`, `crates/tau-ai/src/google.rs`, `crates/tau-ai/src/openai.rs` |
| Provider routing | OpenRouter is a first-class provider | `crates/tau-ai/src/provider.rs`, `crates/tau-ai/tests/provider_http_integration.rs` |
| Session backend | PostgreSQL session backend is implemented | `crates/tau-session/src/session_storage.rs`, `crates/tau-session/Cargo.toml` |
| Onboarding | Guided onboarding crate and command flow are implemented | `crates/tau-onboarding/src/onboarding_command.rs`, `crates/tau-onboarding/src/onboarding_wizard.rs` |
| RL optimizer wiring | PPO/GAE runtime optimization is implemented in training and live RL paths | `crates/tau-coding-agent/src/training_runtime.rs`, `crates/tau-coding-agent/src/live_rl_runtime.rs` |
| Distribution | Dockerfile + release workflow assets are present | `Dockerfile`, `.github/workflows/release.yml`, `scripts/release/render-homebrew-formula.sh`, `scripts/release/generate-shell-completions.sh` |
| Fuzzing | Fuzz harnesses and deterministic fuzz-conformance tests are present | `fuzz/fuzz_targets/`, `scripts/qa/test-fuzz-contract.sh` |
| Log lifecycle | Core log rotation primitives and integration tests are implemented | `crates/tau-core/src/log_rotation.rs`, `crates/tau-runtime/src/observability_loggers_runtime.rs:854`, `crates/tau-gateway/src/gateway_runtime.rs:1098` |
| Operator docs/API docs | Operator deployment/controls and gateway endpoint reference docs are present | `docs/guides/operator-deployment-guide.md`, `docs/guides/operator-control-summary.md`, `docs/guides/gateway-api-reference.md` |
| Dashboard consolidation | Dashboard consolidation is verified via gateway-owned runtime checks and ADR coverage | `scripts/dev/verify-dashboard-consolidation.sh`, `docs/architecture/adr-001-dashboard-consolidation.md` |

## Remaining High-Impact Gaps

None currently identified in this report snapshot. Keep validating periodically as implementation evolves.

## Verification Contract

Run:

```bash
scripts/dev/test-whats-missing.sh
```

The script fails if stale missing-claim markers are reintroduced.
