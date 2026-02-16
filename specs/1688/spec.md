# Issue 1688 Spec

Status: Implemented

Issue: `#1688`  
Milestone: `#21`  
Parent: `#1623`

## Problem Statement

`crates/tau-skills/src/lib.rs` is a large monolith that mixes command-facing API, load/registry orchestration, trust-policy verification, and helper internals in one file. This obscures module boundaries and increases review/maintenance risk.

## Scope

In scope:

- extract loading/registry/cache/lockfile orchestration from `lib.rs` into dedicated module(s)
- extract trust/policy/signature verification orchestration from `lib.rs` into dedicated module(s)
- keep `lib.rs` as a composition surface with unchanged public API
- preserve existing runtime behavior and test outcomes

Out of scope:

- protocol/CLI/wire-format changes
- dependency changes
- feature additions

## Acceptance Criteria

AC-1 (composition surface):
Given `crates/tau-skills/src/lib.rs`,
when reviewing module structure,
then orchestration logic is delegated into focused modules and `lib.rs` primarily composes/re-exports API.

AC-2 (public API stability):
Given existing callers of `tau-skills` public functions,
when compiling and running tests,
then signatures and externally visible behavior remain unchanged.

AC-3 (trust and registry behavior parity):
Given trust/signature and registry resolution flows,
when running targeted tests,
then all pre-existing trust/registry conformance behavior remains green.

AC-4 (regression safety):
Given scoped quality checks,
when running fmt/clippy/tests,
then the crate passes with no new warnings or regressions.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given `tau-skills/src/lib.rs`, when inspected, then `load_registry` and `trust_policy` modules own orchestration bodies while `lib.rs` delegates. |
| C-02 | AC-2 | Conformance | Given existing public APIs (`load_catalog`, `install_skills`, lockfile and registry helpers), when compiling tests, then signatures remain unchanged and call sites compile without adapter shims. |
| C-03 | AC-3 | Integration | Given trust-chain and registry tests, when run, then signed/unsigned, revoked/expired, and checksum/signature cases preserve behavior. |
| C-04 | AC-4 | Regression | Given crate checks, when `cargo fmt --check`, strict clippy, and `cargo test -p tau-skills` run, then all pass. |

## Success Metrics

- `lib.rs` line count reduced with orchestration moved into focused modules
- no public API breakage in `tau-skills`
- targeted + full crate tests remain green
