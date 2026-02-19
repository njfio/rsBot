# Spec #2597 - Task: implement notify watcher + ArcSwap apply path for runtime profile reload

Status: Implemented
Priority: P1
Milestone: M102
Parent: #2596

## Problem Statement
`G16` remains partially unchecked because Tau's runtime heartbeat profile-policy bridge still uses fixed-interval polling and mutable in-task state, rather than file-watch-driven reload with lock-free atomic config swapping.

## Scope
- Replace profile-store polling trigger path with notify-backed file watch events for profile store changes.
- Introduce an `ArcSwap`-backed active profile policy snapshot for lock-free reads and atomic swaps.
- On profile-store change, parse + validate active profile heartbeat policy and apply only valid updates.
- Preserve deterministic bridge outcomes and reason codes: `applied`, `no_change`, `invalid`, `missing_profile`.
- Update `tasks/spacebot-comparison.md` G16 checklist bullets for notify/ArcSwap/watch/swap/log items.

## Out of Scope
- Hot-reload for non-heartbeat policy fields.
- Runtime heartbeat scheduler redesign.
- Profile format migration (`profiles.json` remains source of truth for this slice).

## Acceptance Criteria
- AC-1: Runtime profile-policy bridge watches profile store file changes via `notify` and triggers reload evaluation without relying on periodic polling.
- AC-2: Bridge maintains active heartbeat policy config in `ArcSwap`, swapping atomically only after successful parse/validation.
- AC-3: Invalid profile updates fail closed (last-known-good effective config preserved) while emitting stable `invalid` diagnostics; no-op updates emit deterministic `no_change` diagnostics.
- AC-4: Valid profile updates write refreshed heartbeat `.policy.toml`, emit `applied` diagnostics, and are observable in tests/log output.
- AC-5: `tasks/spacebot-comparison.md` G16 bullets (`notify`, `ArcSwap`, watch/parse/swap/log`) are marked complete after validation.

## Conformance Cases
- C-01 (AC-1, conformance): `cargo test -p tau-coding-agent spec_2597_c01_profile_policy_bridge_notify_events_trigger_reload -- --test-threads=1`
- C-02 (AC-2, conformance): `cargo test -p tau-coding-agent spec_2597_c02_profile_policy_bridge_arcswap_updates_on_valid_change -- --test-threads=1`
- C-03 (AC-3, regression): `cargo test -p tau-coding-agent regression_2597_c03_profile_policy_bridge_invalid_change_preserves_last_good_active_config -- --test-threads=1`
- C-04 (AC-4, regression): `cargo test -p tau-coding-agent regression_2597_c04_profile_policy_bridge_emits_stable_reload_diagnostics -- --test-threads=1`
- C-05 (AC-5, process): G16 checklist bullets updated in `tasks/spacebot-comparison.md`

## Success Signals
- Profile-policy bridge no longer depends on fixed polling interval for change detection.
- Active policy reads are lock-free and atomic across reload updates.
- Reload behavior is deterministic and fail-closed under malformed profile updates.

## Verification Evidence
- `cargo test -p tau-coding-agent 2597_ -- --test-threads=1` => pass (4 passed, 0 failed).
- `cargo test -p tau-coding-agent runtime_profile_policy_bridge::tests:: -- --test-threads=1` => pass (15 passed, 0 failed).
- `cargo fmt --all --check` => pass.
- `cargo clippy -p tau-coding-agent -- -D warnings` => pass.
- `cargo mutants --in-place --in-diff /tmp/issue2597-working.diff -p tau-coding-agent --baseline skip --timeout 180 -- --test-threads=1 runtime_profile_policy_bridge::tests::` => `42 mutants tested in 4m: 12 caught, 30 unviable`, `missed=0`, `timeout=0`.
