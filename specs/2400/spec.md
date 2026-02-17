# Spec: Issue #2400 - Stabilize startup model-catalog remote refresh assertion

Status: Implemented

## Problem Statement
`integration_startup_model_catalog_remote_refresh_is_reported` asserts `entries=1` in startup
diagnostics. The startup model catalog now merges remote payload with built-in entries, so entry
count is environment-dependent and the test fails despite correct behavior.

## Acceptance Criteria

### AC-1 Startup diagnostics assertion is size-agnostic
Given startup model-catalog refresh from a remote URL,
When diagnostics are printed,
Then the test validates remote source markers and a numeric entries field without requiring an
exact hard-coded entry count.

### AC-2 Test still guards remote refresh signal integrity
Given the same startup refresh path,
When diagnostics are emitted,
Then assertions still require `source=remote`, `url=...`, and `entries=<number>` to ensure the
remote refresh path is observable.

### AC-3 Existing startup catalog integration behavior remains unchanged
Given the CLI integration auth-provider suite,
When this test update lands,
Then no production startup model-catalog behavior is modified and the target integration test
passes reliably.

## Scope

### In Scope
- Update one CLI integration assertion in `crates/tau-coding-agent/tests/cli_integration/auth_provider.rs`.
- Add conformance mapping for the updated assertion contract.

### Out of Scope
- Changes to model-catalog merge logic.
- Changes to startup diagnostics format beyond what existing code already emits.
- Provider/runtime behavior changes.

## Conformance Cases

| Case | AC | Tier | Input | Expected |
|---|---|---|---|---|
| C-01 | AC-1 | Integration | Startup with `--model-catalog-url` pointing to mocked `/models.json` | Output contains `model catalog: source=remote url=` and matches `entries=\\d+` |
| C-02 | AC-2 | Functional | Same run as C-01 | Output includes remote URL marker and numeric entry count marker |
| C-03 | AC-3 | Regression | Run targeted CLI integration test post-change | Test passes without touching production startup code paths |

## Success Metrics / Observable Signals
- `integration_startup_model_catalog_remote_refresh_is_reported` passes on current `origin/master`.
- No production source files modified outside test/spec artifacts.
- Scoped `fmt` and `clippy` remain clean for touched crate.
