# M119 - Spacebot G18 Priority Pages in Embedded Dashboard Shell

## Context
G18 still leaves `Priority pages: Overview dashboard, Session viewer, Memory browser, Configuration editor` unchecked. `/dashboard` now serves an embedded shell, but the views are placeholders and do not consume gateway APIs.

## Linked Work
- Epic: #2740
- Story: #2741
- Task: #2742
- Source parity checklist: `tasks/spacebot-comparison.md` (G18 priority pages)

## Scope
- Add API-backed data loading for overview/sessions/memory/configuration in `/dashboard` shell.
- Keep `/webchat` and existing dashboard APIs regression-safe.
- Verify with conformance + regression + live localhost smoke.

## Exit Criteria
- `/dashboard` views load deterministic data from existing gateway endpoints.
- Operators can inspect baseline overview/session/memory/configuration payloads without leaving shell.
- Existing gateway tests remain green.
