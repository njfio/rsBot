# Tau: Gaps, Issues & Improvements (Review #31)

**Date:** 2026-02-21
**HEAD:** `a3428b21` (178 milestones, 434,320 tracked lines, 44 crates, 491 specs)
**Roadmap closure:** 22/23 done, 1 partial, 0 open

This document supersedes Review #30 and refreshes closure status/evidence against the current repository and GitHub issue state.

---

## Table of Contents

1. [Previous Roadmap - Closure Status](#1-previous-roadmap---closure-status)
2. [Remaining Gaps](#2-remaining-gaps)
3. [Stubs & Foundation Code](#3-stubs--foundation-code)
4. [Testing Gaps](#4-testing-gaps)
5. [Architecture Concerns](#5-architecture-concerns)
6. [Repository Hygiene](#6-repository-hygiene)
7. [Security](#7-security)
8. [Documentation & Operational Readiness](#8-documentation--operational-readiness)
9. [Performance & Scalability](#9-performance--scalability)
10. [Prioritized Action Items](#10-prioritized-action-items)

---

## 1. Previous Roadmap - Closure Status

| # | Item | Previous Status | Current Status | Evidence |
|---|------|----------------|----------------|----------|
| 1 | Harden tau-safety tests | Done | **Done** | `crates/tau-safety/src/lib.rs` contains 40 tests |
| 2 | Fix compiler warnings | Done | **Done** | `cargo check -q` passes at HEAD |
| 3 | Add `.env.example` | Done | **Done** | `.env.example` exists |
| 4 | Audit log sanitization | Partial | **Done** | `crates/tau-runtime/src/observability_loggers_runtime.rs` includes `spec_2612_*` redaction tests |
| 5 | Integration test suite | Open | **Done** | `tests/integration/tests/agent_tool_memory_roundtrip.rs` exists (4 integration tests) |
| 6 | Expand under-tested crates | Partial | **Partial** | Direct crate-local test marker counts remain low in this snapshot (`tau-training-proxy` 6, `kamn-core` 4, `kamn-sdk` 5) |
| 7 | Add CHANGELOG.md | Done | **Done** | `CHANGELOG.md` exists |
| 8 | cargo-deny / cargo-audit | Done | **Done** | `deny.toml` + `.github/workflows/security.yml` |
| 9 | Clean stale branches | Open | **Done** | `scripts/dev/stale-merged-branch-prune.sh` exists; remote heads reduced (current: 380) |
| 10 | Add rustfmt.toml | Done | **Done** | `rustfmt.toml` exists |
| 11 | Discord outbound (G10) | Done | **Done** | Mention normalization/chunking paths are present in runtime split modules |
| 12 | Encrypted secrets (G20) | Partial | **Done** | `crates/tau-provider/src/credential_store.rs` with migration and redaction wrappers |
| 13 | Provider failover | Done | **Done** | Fallback routing + circuit-breaker support in provider runtime |
| 14 | Provider rate limiting | Partial | **Done** | `crates/tau-provider/src/client.rs` token-bucket limiter + `spec_2611_*` tests |
| 15 | SQLite memory backend | Done | **Done** | `crates/tau-memory/src/runtime/backend.rs` |
| 16 | Dashboard (G18) | Done | **Done** | Tau ops/dashboard shells and endpoints are shipped under gateway runtime modules |
| 17 | Wire RL training loop | Done | **Done** | Observation/report loop remains implemented (`crates/tau-coding-agent/src/live_rl_runtime.rs`) |
| 18 | OpenTelemetry | Open | **Done** | `crates/tau-runtime/src/observability_loggers_runtime.rs` + `crates/tau-gateway/src/gateway_runtime.rs` OTel export records |
| 19 | Graph visualization (G19) | Open | **Done** | `/api/memories/graph` handlers + memory graph tests in gateway runtime |
| 20 | Multi-process (G1) | Open | **Done** | `crates/tau-agent-core/src/process_types.rs` (`ProcessType`, `ProcessManager`) |
| 21 | External coding agent (G21) | Open | **Done** | `tau-runtime` bridge module + gateway external-agent runtime endpoints |
| 22 | Browser automation | Done | **Done** | Browser automation runtime path remains integrated |
| 23 | Fuzz testing | Done | **Done** | `fuzz/fuzz_targets/` harnesses present |

**Summary:** 22/23 done, 1 partial, 0 open.

---

## 2. Remaining Gaps

### 2.1 Remaining Functional Gaps

1. **Under-tested crate wave follow-through**
   The original expansion issue closed, but direct crate-local test depth remains lower than desired for selected crates (`tau-training-proxy`, `kamn-core`, `kamn-sdk`, and adjacent QA surfaces).

2. **Cortex decision automation (optional roadmap extension)**
   `/cortex/chat` is now LLM-backed, but automated supervisor actuation/routing overrides are still a separate design decision beyond the delivered observer/chat baseline.

### 2.2 M104 Follow-up Issues (Current State)

| Item | Issue | State |
|------|-------|-------|
| Integration test suite bootstrap | #2608 | **Closed** |
| Under-tested crate expansion wave | #2609 | **Closed** |
| Branch hygiene stale cleanup | #2610 | **Closed** |
| Provider-layer token-bucket rate limiting | #2611 | **Closed** |
| Log sanitization audit formalization | #2612 | **Closed** |
| Encrypted secret migration completion | #2613 | **Closed** |
| OpenTelemetry export | #2616 | **Closed** |
| Memory graph visualization (G19) | #2617 | **Closed** |
| Multi-process architecture staging (G1) | #2618 | **Closed** |
| External coding-agent bridge protocol | #2619 | **Closed** |

---

## 3. Stubs & Foundation Code

| Component | Location | Current State | Remaining Stub Surface |
|-----------|----------|---------------|------------------------|
| Deploy endpoint | `crates/tau-gateway/src/gateway_openresponses/deploy_runtime.rs` | Request/stop state is persisted deterministically | Does not spawn/terminate OS processes |
| Stop endpoint | `crates/tau-gateway/src/gateway_openresponses/deploy_runtime.rs` | Agent state transitions to stopped and persists | No process-kill orchestration layer |
| RL weight updates | `crates/tau-coding-agent/src/live_rl_runtime.rs` | Captures rollouts and emits optimization reports | Does not write updated model weights (by design) |

---

## 4. Testing Gaps

### 4.1 Under-Tested Areas (Current Snapshot)

| Area | Current Signal | Recommendation |
|------|----------------|----------------|
| Integration breadth | `tests/integration/` contains one file with 4 tests | Expand scenario count for channel routing/compaction/delegation |
| tau-diagnostics | 6 direct test markers | Add edge-case coverage for audit aggregation and telemetry compatibility |
| tau-training-proxy | 6 direct test markers | Add persistence/recovery and malformed-rollout cases |
| kamn-core | 4 direct test markers | Add boundary tests for identity/auth edge cases |
| kamn-sdk | 5 direct test markers | Add contract and integration fixture coverage for SDK call paths |

### 4.2 Missing/Light Categories

| Category | Current State | Recommendation |
|----------|---------------|----------------|
| Property-based tests | Minimal usage in core roadmap surfaces | Add `proptest` for ranking/decay/token-limit math |
| Concurrency stress | Targeted coverage exists but thin for some paths | Add race-oriented tests around memory writes and process supervision |
| Deploy runtime integration | Deterministic state tests only | Add process-lifecycle integration when deploy runtime is upgraded |

---

## 5. Architecture Concerns

### 5.1 Gateway Module Size (Improved, Still a Hotspot)

`crates/tau-gateway/src/gateway_openresponses.rs` is now 1,973 lines (down from earlier >4k snapshots), but remains a concentration point. Continued domain extraction should keep this file trending downward.

### 5.2 Single-Binary Runtime Limits

The current process model supports branch/worker process semantics in one runtime, but crash/resource isolation remains bounded by a single process model.

### 5.3 Dashboard Stack vs PRD

The Leptos PRD exists at `specs/tau-ops-dashboard-prd.md`, while the shipped dashboard shell remains HTML/JS served from gateway runtime modules. This is a roadmap decision point, not a current defect.

---

## 6. Repository Hygiene

| Item | Status | Evidence |
|------|--------|----------|
| Stale branch hygiene | **Improved** | `scripts/dev/stale-merged-branch-prune.sh`; remote branch count currently 380 |
| Dependabot backlog | **Open** | Open dependency PRs #2710-#2714 |
| CONTRIBUTING.md | **Missing** | No root `CONTRIBUTING.md` currently tracked |
| SECURITY.md | **Missing** | No root `SECURITY.md` currently tracked |

---

## 7. Security

| Item | Status | Evidence |
|------|--------|----------|
| Encrypted credential store | **Done** | `crates/tau-provider/src/credential_store.rs` |
| Decrypted secret redaction wrappers | **Done** | `DecryptedSecret` `Debug/Display` emit `[REDACTED]` |
| SSRF protections | **Done** | Existing gateway safety guards remain in tree |
| Secret-leak detection controls | **Done** | `crates/tau-safety/src/lib.rs` pattern + policy controls |
| Log sanitization audit formalization | **Done** | `spec_2612_*` coverage in observability logger runtime |
| Key rotation CLI | **Missing** | No explicit key-rotation command contract in current CLI |

---

## 8. Documentation & Operational Readiness

| Item | Status | Evidence |
|------|--------|----------|
| Operator deployment guide | **Done** | `docs/guides/operator-deployment-guide.md` |
| API reference | **Done** | `docs/guides/gateway-api-reference.md` |
| Deployment ops guide | **Done** | `docs/guides/deployment-ops.md` |
| Runbook ownership map | **Done** | `docs/guides/runbook-ownership-map.md` |
| Architecture ADR trail | **Done** | `docs/architecture/adr-00*.md` set |
| High-level dependency graph doc | **Partial** | ADRs exist; consolidated crate dependency diagram is still not published |

---

## 9. Performance & Scalability

| Concern | Current State | Recommendation |
|---------|---------------|----------------|
| Gateway runtime hotspot | Reduced to 1,973-line core module | Continue split-by-domain extraction |
| FileMemoryStore query cost | SQLite backend exists; file-backed mode still linear scans | Prefer SQLite for larger deployments |
| Memory graph rendering scale | Works for current dashboard shell usage | Add heavier-load profiling if node count targets increase |
| Provider burst control | Provider-layer token bucket shipped | Add operational dashboards for saturation visibility |

---

## 10. Prioritized Action Items

### P0 - Next High-Impact Closures

1. **Upgrade deploy/stop from state transitions to process lifecycle control** (spawn/terminate with supervision).
2. **Decide cortex automation scope** (keep advisory-only vs add supervisor-routing actions).

### P1 - Quality and Maintainability

3. **Expand under-tested crate coverage** with explicit target thresholds and conformance mapping.
4. **Continue gateway runtime modularization** until hotspot pressure is reduced further.
5. **Publish crate dependency architecture diagram** for onboarding and ops.
6. **Add root `CONTRIBUTING.md` and `SECURITY.md`** for contributor/security process clarity.

### P2 - Medium-Term Enhancements

7. **Add key-rotation CLI flow** for encrypted credential store operations.
8. **Grow property/concurrency testing** for ranking/compaction/process paths.
9. **Finalize dashboard stack direction** (maintain HTML/JS shell vs Leptos migration).

---

## Summary

This refresh removes stale follow-up status drift: all prior M104 follow-up issues (`#2608`, `#2609`, `#2610`, `#2611`, `#2612`, `#2613`, `#2616`, `#2617`, `#2618`, `#2619`) are now reflected as **Closed**. The roadmap table is updated to current implementation evidence, and the main remaining gap is quality depth in under-tested crates rather than missing foundational features.
