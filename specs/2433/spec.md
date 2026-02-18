# Spec #2433

Status: Implemented
Milestone: specs/milestones/m73/index.md
Issue: https://github.com/njfio/Tau/issues/2433

## Problem Statement

`tau-tools` currently fails bash conformance/regression tests in two behavior
areas:

- policy mode/reason-code signaling on successful or fail-closed command paths
- rate-limit enforcement semantics for throttling, per-principal isolation, and
  reset after throttle window

These failures block crate-level validation and reduce operator confidence in
bash safety controls.

## Scope

In scope:

- `crates/tau-tools` bash execution policy/reason signaling fixes.
- `crates/tau-tools` bash rate-limit bookkeeping/enforcement fixes.
- Conformance/regression tests that map directly to ACs.

Out of scope:

- New provider features.
- Broader CLI/runtime refactors.

## Acceptance Criteria

- AC-1: Given bash execution is allowed in default policy mode, when the tool
  succeeds, then output includes `policy_mode="none"` and
  `policy_reason_code="none"`.
- AC-2: Given bash execution requires explicit policy and no policy exists, when
  the tool is invoked, then it fails closed with
  `policy_reason_code="sandbox_policy_required"`.
- AC-3: Given bash rate limiting is configured for one request per window,
  when the same principal issues a second request in the same window, then the
  second request is denied and throttle trace metadata is populated.
- AC-4: Given bash rate limiting is configured, when different principals issue
  requests, then throttling state is isolated per principal.
- AC-5: Given a principal was throttled, when the rate-limit window elapses,
  then the next request from that principal succeeds.

## Conformance Cases

- C-01 (AC-1, conformance): `bash_tool_runs_command` asserts
  `policy_mode="none"` and `policy_reason_code="none"` on success.
- C-02 (AC-2, conformance):
  `regression_bash_tool_required_policy_mode_fails_closed_with_reason_code`
  asserts `policy_reason_code="sandbox_policy_required"`.
- C-03 (AC-3, conformance):
  `regression_bash_tool_rate_limit_trace_reports_throttle_details` asserts
  second call is denied and trace includes throttle details.
- C-04 (AC-4, conformance):
  `integration_bash_tool_rate_limit_isolated_per_principal` asserts throttling
  applies independently per principal.
- C-05 (AC-5, conformance):
  `functional_bash_tool_rate_limit_resets_after_window` asserts post-window
  request succeeds.

## Success Metrics / Observable Signals

- Targeted red/green loop completes with all C-01..C-05 passing.
- `cargo test -p tau-tools bash_tool -- --nocapture` passes.
- `cargo fmt --check` and `cargo clippy -p tau-tools --no-deps -- -D warnings`
  pass.
