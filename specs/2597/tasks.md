# Tasks #2597

1. [x] T1 (tests/red): add failing tests for notify-driven reload trigger, ArcSwap active-config swap, invalid fail-closed preservation, and stable diagnostics.
2. [x] T2 (impl): add notify watcher state and event drain logic for profile-store updates.
3. [x] T3 (impl): add ArcSwap active-policy snapshot and atomic apply path.
4. [x] T4 (green): run conformance C-01..C-04 and update G16 checklist bullets (C-05).
5. [x] T5 (verify): run scoped fmt/clippy/tests and mutation-in-diff for touched crate.
6. [x] T6 (process): hand off closure evidence packaging to #2598.

Evidence:
- RED: `cargo test -p tau-coding-agent 2597_ -- --test-threads=1` initially failed on `spec_2597_c01`/`spec_2597_c02` before watcher hardening.
- GREEN: `cargo test -p tau-coding-agent 2597_ -- --test-threads=1` => pass (4/4).
- Stability: `spec_2597_c01_profile_policy_bridge_notify_events_trigger_reload` passed in 5 consecutive runs.
- Quality: `cargo fmt --all --check` and `cargo clippy -p tau-coding-agent -- -D warnings` => pass.
- Mutation: `cargo mutants --in-place --in-diff /tmp/issue2597-working.diff -p tau-coding-agent --baseline skip --timeout 180 -- --test-threads=1 runtime_profile_policy_bridge::tests::` => `42 tested, 12 caught, 30 unviable, 0 missed, 0 timeout`.
- Checklist: G16 notify/ArcSwap/watch/swap/log bullets marked complete in `tasks/spacebot-comparison.md`.
