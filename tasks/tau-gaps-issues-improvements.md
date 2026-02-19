# Tau: Gaps, Issues & Improvements (Revalidated)

**Revalidation date:** 2026-02-19  
**Milestone:** M104 (`specs/milestones/m104/index.md`)  
**Primary task:** #2607

This document supersedes the earlier 2026-02-10 snapshot and provides an evidence-backed status for every roadmap item (1..23).

## Method

Validation used repository and issue evidence plus targeted commands:

- `cargo check -p tau-coding-agent`
- `cargo test -p tau-safety`
- `rg` validation across runtime/provider/multi-channel crates and workflows
- `gh issue` / milestone inspection for completed and follow-up work

## Prioritized Roadmap Revalidation

| # | Item | Status | Evidence | Action / Issue |
|---|------|--------|----------|----------------|
| 1 | Harden `tau-safety` tests | **Done** | `crates/tau-safety/src/lib.rs` now has 40 tests; `cargo test -p tau-safety` passes | Completed in #2607 |
| 2 | Fix 4 compiler warnings (`tau-coding-agent`) | **Done** | `cargo check -p tau-coding-agent` shows clean build with no warnings | No further action |
| 3 | Add `.env.example` | **Done** | `.env.example` added with provider/gateway/memory/multi-channel baseline vars | Completed in #2607 |
| 4 | Audit log sanitization | **Partial** | Secret/redaction systems exist (`crates/tau-safety/src/lib.rs`, `crates/tau-provider/src/credential_store.rs`), but no formalized repo-wide log-leak audit gate | Follow-up: #2612 |
| 5 | Add integration test suite (`tests/integration`) | **Open** | Root `tests/integration` does not exist | Follow-up: #2608 |
| 6 | Expand under-tested crates | **Partial** | `tau-safety` raised to 40 tests; several crates still low (for example `tau-diagnostics`, `tau-training-proxy`) | Follow-up: #2609 |
| 7 | Add `CHANGELOG.md` | **Done** | `CHANGELOG.md` added | Completed in #2607 |
| 8 | Add `cargo-deny` / `cargo-audit` in CI | **Done** | `deny.toml` exists; security workflow already runs audit/deny checks (`.github/workflows/security.yml`) | No further action |
| 9 | Clean stale branches | **Open** | Branch cleanup process not yet formalized in milestone flow | Follow-up: #2610 |
| 10 | Add `rustfmt.toml` | **Done** | `rustfmt.toml` added | Completed in #2607 |
| 11 | Complete Discord outbound (G10) | **Done** | Discord outbound send + file + reaction paths are implemented and tested (`crates/tau-multi-channel/src/multi_channel_outbound.rs`) | No further action |
| 12 | Add encrypted secrets (G20) | **Partial** | Keyed encryption exists in credential store (`crates/tau-provider/src/credential_store.rs`); broader migration remains | Follow-up: #2613 |
| 13 | Add provider failover | **Done** | Fallback routing + circuit breaker implemented (`crates/tau-provider/src/fallback.rs`) | No further action |
| 14 | Add rate limiting for outbound provider calls | **Partial** | Gateway rate limiting exists (`crates/tau-gateway/src/gateway_openresponses.rs`), but dedicated outbound token-bucket control is not isolated as a provider-layer feature | Follow-up: #2611 |
| 15 | Add SQLite memory backend | **Done** | SQLite backend is implemented (`crates/tau-memory/src/runtime/backend.rs`) | No further action |
| 16 | Build real dashboard (G18) | **Done** | Gateway webchat now includes a production dashboard operator surface with authenticated dashboard health/widgets/timeline/alerts/actions + live polling (`crates/tau-gateway/src/gateway_openresponses/webchat_page.rs`) | Completed in #2614 |
| 17 | Wire RL training loop to live decisions | **Done** | Live RL runtime bridge now captures agent events into rollouts/spans and runs scheduled PPO/GAE updates (`crates/tau-coding-agent/src/live_rl_runtime.rs`) | Completed in #2615 |
| 18 | Add OpenTelemetry | **Open** | No active OpenTelemetry export path in runtime/gateway | Follow-up: #2616 |
| 19 | Add graph visualization (G19) | **Open** | No memory graph API/UI visualization path shipped | Follow-up: #2617 |
| 20 | Multi-process architecture (G1) | **Open** | Current runtime remains single-loop oriented for core interaction path | Follow-up: #2618 |
| 21 | External coding-agent protocol (G21) | **Open** | No finalized external coding-agent bridge protocol/session manager | Follow-up: #2619 |
| 22 | Browser automation live integration | **Done** | Live browser automation runner and Playwright CLI execution path are implemented (`crates/tau-browser-automation/src/browser_automation_live.rs`) | No further action |
| 23 | Add fuzz testing | **Done** | Fuzz harness and targets exist (`fuzz/fuzz_targets/*.rs`) | No further action |

## Contract Artifacts for This Revalidation Slice

- Milestone index: `specs/milestones/m104/index.md`
- Task spec: `specs/2607/spec.md`
- Task plan: `specs/2607/plan.md`
- Task tasks: `specs/2607/tasks.md`

## Immediate Outcomes in #2607

- Expanded `tau-safety` from 9 to 40 tests with added obfuscation/regression coverage.
- Added missing baseline artifacts: `.env.example`, `CHANGELOG.md`, `rustfmt.toml`.
- Created explicit follow-up implementation issues for all remaining non-trivial open roadmap items.

## Follow-up Issue Index (Open)

- #2608 Integration suite bootstrap (`tests/integration`)
- #2609 Under-tested crate expansion wave
- #2610 Safe stale-branch cleanup process
- #2611 Provider-layer token-bucket rate limiting
- #2612 Log sanitization audit formalization
- #2613 Encrypted secret migration completion
- #2616 OpenTelemetry export path
- #2617 Memory graph visualization (G19)
- #2618 Multi-process architecture staging (G1)
- #2619 External coding-agent bridge protocol (G21)
