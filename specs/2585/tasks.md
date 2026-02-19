# Tasks #2585

1. [x] T1 (verify): run mapped #2584 conformance/regression commands.
2. [x] T2 (verify): run scoped lint/format + crate tests.
3. [x] T3 (mutation): run `cargo mutants --in-diff` for touched paths.
4. [x] T4 (live validation): run sanitized live smoke and capture summary.
5. [x] T5 (process): update issue logs and finalize evidence package.

## Evidence

- Conformance regression commands from `specs/2584/spec.md` C-01..C-05 passed.
- Scoped quality gates passed:
  - `cargo fmt --check`
  - `cargo clippy -p tau-memory -p tau-tools -p tau-runtime -- -D warnings`
- Mutation in diff:
  - `cargo mutants --in-diff /tmp/issue2584.diff -p tau-memory -p tau-tools -p tau-runtime`
  - Outcome: `Diff changes no Rust source files` (docs-only diff; no mutable Rust targets in scope).
- Live smoke:
  - `TAU_PROVIDER_KEYS_FILE=<sanitized temp keys> ./scripts/dev/provider-live-smoke.sh`
  - Summary: `ok=3 skipped=5 failed=0`
- Process logs:
  - `#2584` comment: `https://github.com/njfio/Tau/issues/2584#issuecomment-3925491250`
  - `#2585` comment: `https://github.com/njfio/Tau/issues/2585#issuecomment-3925491567`
