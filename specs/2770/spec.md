# Spec: Issue #2770 - G10 serenity dependency and tau-discord-runtime foundation

Status: Reviewed

## Problem Statement
G10 still has unresolved foundational work: no workspace `serenity` dependency and no dedicated `tau-discord-runtime` crate/module boundary. This blocks full Discord parity roadmap closure and prevents clean ownership of Discord-specific runtime logic.

## Acceptance Criteria

### AC-1 Workspace dependency includes serenity with documented rationale
Given workspace manifests and dependency policy,
When #2770 is implemented,
Then `serenity` is added in an approved scope with justification and no unrelated dependency churn.

### AC-2 Discord runtime foundation crate/module exists and compiles
Given current workspace layout,
When #2770 is implemented,
Then a `tau-discord-runtime` crate (or approved equivalent module boundary) exists, is wired into workspace config, and compiles with existing pipelines.

### AC-3 Integration path preserves current Discord functionality
Given existing Discord adapter capabilities,
When the new foundation is introduced,
Then existing behavior (polling/backfill/streaming/reaction/send-file/thread/typing) remains green in scoped regression tests.

### AC-4 Roadmap evidence is updated
Given implementation completion,
When verification passes,
Then `tasks/spacebot-comparison.md` marks the remaining G10 rows with issue evidence.

## Scope

### In Scope
- Workspace dependency declaration updates for `serenity`.
- New crate/module scaffolding and workspace wiring for Discord runtime boundary.
- Regression validation and checklist evidence updates.

### Out of Scope
- G20 encrypted store dependencies (`aes-gcm`, `redb`).
- G23 Fly.io CI integration.
- Broad protocol redesign.

## Conformance Cases
- C-01 (conformance): dependency addition is present and scoped to intended manifests.
- C-02 (integration): new Discord runtime crate/module compiles in workspace checks.
- C-03 (regression): existing Discord/multi-channel behavior tests remain green.
- C-04 (docs): G10 unchecked rows updated with `#2770` evidence.

## Success Metrics / Observable Signals
- Repository has clear Discord runtime boundary and dependency contract.
- No regression in current Discord adapter feature tests.
- G10 checklist has no remaining unchecked implementation-foundation rows.

## Approval Gate
This task requires explicit user approval before implementation because it introduces a new dependency (`serenity`) per `AGENTS.md` ask-first rules.
