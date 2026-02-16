# Plan #2171

Status: Implemented
Spec: specs/2171/spec.md

## Approach

1. Add RED wave-9 marker assertions to
   `scripts/dev/test-split-module-rustdoc.sh`.
2. Run guard script and capture expected failure.
3. Add concise rustdoc comments to wave-9 session/memory helper modules.
4. Run guard, scoped checks, and targeted tests for both crates.

## Affected Modules

- `specs/milestones/m36/index.md`
- `specs/2171/spec.md`
- `specs/2171/plan.md`
- `specs/2171/tasks.md`
- `scripts/dev/test-split-module-rustdoc.sh`
- `crates/tau-session/src/session_locking.rs`
- `crates/tau-session/src/session_storage.rs`
- `crates/tau-session/src/session_integrity.rs`
- `crates/tau-memory/src/runtime/backend.rs`

## Risks and Mitigations

- Risk: marker assertions become brittle.
  - Mitigation: assert stable phrases tied to API intent names.
- Risk: docs edit accidentally changes behavior.
  - Mitigation: docs-only line additions plus scoped compile/test matrix.

## Interfaces and Contracts

- Guard:
  `bash scripts/dev/test-split-module-rustdoc.sh`
- Compile:
  `cargo check -p tau-session --target-dir target-fast`
  `cargo check -p tau-memory --target-dir target-fast`
- Targeted tests:
  `cargo test -p tau-session integration_sqlite_backend_auto_imports_legacy_jsonl_snapshot --target-dir target-fast`
  `cargo test -p tau-session unit_acquire_lock_creates_missing_parent_directories --target-dir target-fast`
  `cargo test -p tau-memory integration_memory_store_imports_legacy_jsonl_into_sqlite --target-dir target-fast`
  `cargo test -p tau-memory functional_memory_store_defaults_to_sqlite_backend_for_directory_roots --target-dir target-fast`

## ADR References

- Not required.
