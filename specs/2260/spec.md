# Spec #2260

Status: Implemented
Milestone: specs/milestones/m46/index.md
Issue: https://github.com/njfio/Tau/issues/2260

## Problem Statement

Onboarding includes first-run detection and wizard plan helpers, but the guided flow
is not fully enforced at command orchestration boundaries. In particular, selected
wizard workspace is not applied consistently to onboarding persistence/report paths,
and interactive flow behavior is not covered by deterministic end-to-end command
tests.

## Scope

In scope:

- Complete guided onboarding command flow with deterministic prompt-driven execution
  tests.
- Ensure wizard-selected workspace is applied to onboarding persistence root.
- Preserve non-interactive behavior and existing first-run detection semantics.

Out of scope:

- New onboarding UX surfaces outside CLI wizard prompts.
- Major redesign of onboarding report schema.
- Provider auth backend implementations (handled by separate issues).

## Acceptance Criteria

- AC-1: Given interactive onboarding with scripted prompt responses, when user
  confirms and provides provider/auth/model/workspace choices, then command execution
  succeeds deterministically without stdin dependency in tests.
- AC-2: Given interactive onboarding where user selects a workspace root, when
  onboarding runs, then profile store/release channel/baseline/report paths resolve
  under the selected workspace `.tau` root.
- AC-3: Given interactive onboarding where user declines initial confirmation, when
  command runs, then onboarding is canceled and no onboarding report file is written.
- AC-4: Given non-interactive onboarding mode, when command runs, then existing
  behavior and path selection remain unchanged.

## Conformance Cases

- C-01 (AC-1, functional): deterministic scripted interactive flow executes
  `execute_onboarding_command` equivalent path and emits summary lines.
- C-02 (AC-2, integration): interactive selected workspace persists onboarding assets
  under chosen workspace `.tau` root.
- C-03 (AC-3, regression): interactive cancel path does not write onboarding report or
  mutate onboarding stores.
- C-04 (AC-4, regression): non-interactive onboarding report path remains under
  `resolve_tau_root(cli)`.

## Success Metrics / Observable Signals

- Guided flow has deterministic command-level tests that do not depend on stdin.
- Workspace selection from wizard is reflected in persisted onboarding artifacts.
- Existing non-interactive tests remain green.
