# Plan: Issue #2420 - Slack bridge message coalescing

## Approach
1. Add conformance tests first (RED) for coalescing window gating and batching semantics.
2. Introduce deterministic queue helper that selects a coalesced run segment without mutating non-eligible tail events.
3. Integrate helper into `try_start_queued_runs` and keep run/task pipeline unchanged beyond merged event payload text.
4. Add Slack coalescing config field through CLI args and onboarding transport config to runtime config.
5. Run scoped quality/test gates (GREEN).

## Affected Modules
- `crates/tau-slack-runtime/src/slack_runtime.rs`
- `crates/tau-slack-runtime/src/slack_runtime/tests.rs`
- `crates/tau-onboarding/src/startup_transport_modes.rs`
- `crates/tau-onboarding/src/startup_transport_modes/tests.rs`
- `crates/tau-coding-agent/src/startup_transport_modes.rs`
- `crates/tau-cli/src/cli_args/gateway_daemon_flags.rs`
- `crates/tau-coding-agent/src/tests.rs` (CLI defaults fixture)

## Risks / Mitigations
- Risk: unintended batching across unrelated thread/user contexts.
  - Mitigation: strict contiguous eligibility checks by user + reply thread key + time gap.
- Risk: delayed responses feel sluggish.
  - Mitigation: bounded default window (2000ms) and configurable override.
- Risk: startup wiring drift.
  - Mitigation: explicit onboarding config tests asserting default and override propagation.

## Interfaces / Contracts
- Add `slack_coalescing_window_ms` to CLI/onboarding/runtime config structs.
- Add queue coalescing helpers in Slack runtime with deterministic behavior.

## ADR
- Not required; no dependency, protocol, or architecture-level change.
