# Issue 1684 Spec

Status: Implemented

Issue: `#1684`  
Milestone: `#21`  
Parent: `#1637`

## Problem Statement

`crates/tau-provider/src/auth_commands_runtime.rs` includes shared launch/runtime helpers and provider-specific login-ready flows (OpenAI/Anthropic/Google) in one monolithic file. This makes provider behavior boundaries hard to reason about and increases maintenance risk.

## Scope

In scope:

- extract shared auth runtime helper core into a dedicated module
- extract provider-specific login backend-ready flows into per-provider modules
- preserve existing `/auth` command compatibility and output behavior
- keep main runtime file as orchestration/composition surface

Out of scope:

- auth behavior changes
- CLI contract changes
- dependency changes

## Acceptance Criteria

AC-1 (shared core extraction):
Given auth runtime helpers,
when reviewing module layout,
then shared launch/redaction/build-spec helpers live in a dedicated shared-core module.

AC-2 (provider extraction):
Given provider-specific login-ready flows,
when reviewing module layout,
then Google/OpenAI/Anthropic runtime branches are implemented in dedicated modules.

AC-3 (command compatibility):
Given existing auth command inputs,
when running tests,
then behavior and command compatibility remain unchanged.

AC-4 (regression safety):
Given scoped quality checks,
when running tests/lints/format checks,
then all pass with no new warnings.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given `auth_commands_runtime.rs`, when inspected, then shared helper implementations are moved to `auth_commands_runtime/shared_runtime_core.rs`. |
| C-02 | AC-2 | Functional | Given provider login-ready functions, when inspected, then implementations are hosted in `google_backend.rs`, `openai_backend.rs`, and `anthropic_backend.rs`. |
| C-03 | AC-3 | Integration | Given `tau-provider` auth command tests, when run, then login/status/logout/matrix behavior remains parity. |
| C-04 | AC-4 | Regression | Given scoped checks, when running `cargo test -p tau-provider`, strict clippy, and fmt, then all pass. |

## Success Metrics

- `auth_commands_runtime.rs` reduced and focused on orchestration
- shared and provider-specific runtime concerns are explicitly separated
- no auth command regressions in `tau-provider` tests
