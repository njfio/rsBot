# M73 — Tau Tools Bash Reliability and Policy Signals

Milestone: [GitHub milestone #73](https://github.com/njfio/Tau/milestone/73)

## Objective

Restore deterministic `tau-tools` bash behavior for policy reason signaling and
rate-limit enforcement so crate-level validation is stable and auditable.

## Scope

- Fix bash-tool output metadata so policy mode and reason-code fields are
  consistently populated on allow and deny paths.
- Fix rate-limit enforcement behavior for same-principal throttling,
  cross-principal isolation, and post-window reset behavior.
- Lock behavior with spec-mapped conformance/regression tests.

## Out of Scope

- Non-bash tool behavior changes.
- Provider/runtime architecture changes outside `crates/tau-tools`.

## Linked Hierarchy

- Epic: #2431
- Story: #2432
- Task: #2433
- Subtask: #2434
