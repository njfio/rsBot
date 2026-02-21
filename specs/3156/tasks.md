# Tasks: Issue #3156 - Property invariants for rate-limit reset, disable, and payload contracts

- [x] T1 (RED): add failing property tests for reset and disabled-limiter invariants (C-01..C-04).
- [x] T2 (RED): add failing property test for gate payload contract invariants across reject/defer behavior (C-05).
- [x] T3 (GREEN): refine assertions/generators to satisfy intended invariants without weakening contract checks.
- [x] T4 (VERIFY): run `cargo test -p tau-tools spec_3156 -- --test-threads=1`, regression `spec_3152`, `cargo fmt --check`, and `cargo clippy -p tau-tools -- -D warnings`.
