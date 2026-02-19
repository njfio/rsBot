# Tasks #2544

1. T1 (tests): reproduce `spec_2465/spec_2487` failures and add failing watcher-miss fallback tests.
2. T2 (impl): add deterministic policy fingerprint fallback in hot-reload evaluation logic.
3. T3 (regression): preserve pending-reload fail-closed behavior when watcher context is absent.
4. T4 (verify): run fmt/clippy/scoped `tau-runtime` suite and workspace `cargo test -j 1`.
5. T5 (docs/process): capture verification evidence and update issue process logs.
6. T6 (mutation hardening): add pending-reload short-circuit regression coverage until `cargo mutants --in-diff` has zero missed mutants.
