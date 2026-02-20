# M118 - Spacebot G18 Embedded SPA Shell from Gateway

## Context
`tasks/spacebot-comparison.md` still leaves the `Serve embedded SPA from gateway` G18 pathway item unresolved. Tau currently serves `/webchat` as an inline HTML shell, but there is no dedicated embedded dashboard shell route for future SPA expansion.

## Linked Work
- Epic: #2736
- Story: #2737
- Task: #2738
- Source parity checklist: `tasks/spacebot-comparison.md` (G18 Serve embedded SPA from gateway)

## Scope
- Add a gateway-served embedded dashboard shell route.
- Provide baseline shell navigation placeholders for overview/sessions/memory/configuration.
- Preserve existing `/webchat` and dashboard API behavior.

## Exit Criteria
- `/dashboard` returns deterministic embedded shell HTML.
- Shell includes baseline sections for overview/sessions/memory/configuration.
- Existing gateway webchat/dashboard tests remain green.
