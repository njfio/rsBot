# M123 - Spacebot G10 Discord History Backfill (100)

## Context
`tasks/spacebot-comparison.md` still leaves G10 `Message history backfill (up to 100 messages before trigger)` unchecked. Tau Discord polling currently ingests only the latest poll batch and does not explicitly backfill up to 100 messages on first-run trigger.

## Linked Work
- Epic: #2756
- Story: #2757
- Task: #2758
- Source parity checklist: `tasks/spacebot-comparison.md` (G10)

## Scope
- Add first-run Discord polling history backfill behavior up to 100 messages per configured ingress channel.
- Preserve incremental cursor-based polling behavior after first-run.
- Preserve guild allowlist filtering semantics during backfill.
- Add conformance/regression coverage and checklist evidence.

## Exit Criteria
- First-run/no-cursor Discord polling ingests up to 100 recent messages before trigger.
- Subsequent polling cycles ingest only messages newer than stored cursor.
- Guild allowlist filtering continues to be enforced during backfill.
- Scoped fmt/clippy/tests and localhost live validation are green.
