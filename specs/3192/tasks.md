# Tasks: Issue #3192 - correct inaccurate PPO unresolved-gap claim in whats-missing report

- [x] T1 (RED): update `scripts/dev/test-whats-missing.sh` to require corrected PPO marker and reject stale marker; run expecting failure.
- [x] T2 (GREEN): update `tasks/whats-missing.md` PPO language to match implemented behavior and rerun conformance.
- [x] T3 (VERIFY): run `scripts/dev/test-whats-missing.sh`, `cargo fmt --check`, `cargo clippy -- -D warnings`.
