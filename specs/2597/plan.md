# Plan #2597

## Approach
1. Add a profile-store watcher state to `runtime_profile_policy_bridge` using `notify` and `tokio::mpsc` event fan-in.
2. Introduce a compact active-policy snapshot struct and hold it in `ArcSwap` for lock-free read and atomic swap semantics.
3. Refactor evaluation flow so file-change events drive parse/validate/apply; keep deterministic no-change and invalid handling.
4. Preserve existing bridge reason-code contract and extend tests for watcher + ArcSwap behavior.
5. Update G16 checklist bullets after conformance verification.

## Affected Modules
- `crates/tau-coding-agent/src/runtime_profile_policy_bridge.rs`
- `tasks/spacebot-comparison.md`

## Risks & Mitigations
- Risk: notify watcher edge cases may miss updates on some filesystems.
  - Mitigation: include deterministic fallback fingerprint detection when watcher channel is unavailable/disconnected.
- Risk: introducing ArcSwap may regress existing bridge outcomes.
  - Mitigation: preserve stable reason-code assertions and add dedicated regression tests for no-change/invalid/apply behavior.

## Interfaces / Contracts
- Bridge public lifecycle (`start_runtime_heartbeat_profile_policy_bridge` + handle shutdown) remains unchanged.
- Outcome reason-code strings remain stable (`profile_policy_bridge_applied|no_change|invalid|missing_profile`).
