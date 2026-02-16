## `allow(...)` Audit - Wave 2

Date: 2026-02-16  
Issue: #2203

### Scope

- Inventory active `allow(...)` usages under `crates/`.
- Remove safe stale suppressions where possible.
- Document rationale for retained suppressions.

### Inventory Method

Command used:

```bash
rg -n "allow\(" crates -g '*.rs'
```

### Summary

- Baseline before this change: `3` allow pragmas.
- Current after this change: `2` allow pragmas.
- Lint kinds observed after cleanup: `dead_code`, `unused_imports` (both scoped with `cfg_attr`).

### Removal Completed

- Removed stale `#[allow(dead_code)]` from:
  - `crates/tau-algorithm/src/ppo.rs`
- Change made:
  - Deleted unused `_assert_step_shape(...)` test helper that had no call sites.
- Result:
  - No behavior change, one suppression removed.

### Current Remaining Pragmas (2)

1. `crates/tau-diagnostics/src/lib.rs:1311`
   Attribute: `#[cfg_attr(not(test), allow(dead_code))]`
   Target: `run_doctor_checks`
   Rationale: retained as a compatibility/testing entrypoint; dead-code suppression applies only to non-test builds.
2. `crates/tau-coding-agent/src/main.rs:1`
   Attribute: `#![cfg_attr(test, allow(unused_imports))]`
   Target: crate root
   Rationale: retained for test-mode compilation path where top-level imports are intentionally broader than direct test references.

### Validation

- `cargo check -p tau-algorithm --target-dir target-fast`
- `cargo test -p tau-algorithm ppo --target-dir target-fast`
- `cargo clippy -p tau-algorithm --target-dir target-fast -- -D warnings`

### Next Wave

- Re-evaluate whether `run_doctor_checks` can be referenced directly in non-test paths to eliminate `cfg_attr(...allow(dead_code))`.
- Re-check `main.rs` test import policy after startup/runtime import consolidation.
