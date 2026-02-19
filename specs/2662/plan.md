# Plan: Issue #2662

## Approach
1. Add failing tests for Discord mention normalization and chunk cap behavior.
2. Implement mention normalization helper(s) in `multi_channel_live_ingress` and wire into Discord event parsing.
3. Keep fallback deterministic: unresolved mentions stay raw.
4. Re-run scoped checks and update roadmap evidence.

## Affected Modules
- `crates/tau-multi-channel/src/multi_channel_live_ingress.rs`
- `crates/tau-multi-channel/src/multi_channel_outbound.rs`
- `tasks/spacebot-comparison.md`

## Risks / Mitigations
- Risk: over-eager replacement modifies unrelated text.
  - Mitigation: only replace exact `<@ID>` / `<@!ID>` tokens built from mention metadata IDs.
- Risk: display-name source ambiguity.
  - Mitigation: deterministic precedence (`member.nick` -> `global_name` -> `username`).
- Risk: chunk cap regression for Discord.
  - Mitigation: explicit test with `max_chars > 2000` and long payload.

## Interfaces / Contracts
- No external API schema changes.
- Internal parsing behavior change only for normalized Discord text content.
