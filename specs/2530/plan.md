# Plan #2530

## Approach
1. Implement RED tests for missing Discord and Slack send-file paths.
2. Add Discord send-file dispatch in `tau-multi-channel` outbound provider path.
3. Add Slack send-file directive extraction and Slack API upload v2 helper flow.
4. Preserve existing Telegram send-file and unsupported-transport behavior.
5. Validate with scoped and full gates.

## Affected Modules
- `crates/tau-multi-channel/src/multi_channel_outbound.rs`
- `crates/tau-multi-channel/src/multi_channel_runtime/tests.rs` (if reason-code mapping changes)
- `crates/tau-slack-runtime/src/slack_runtime.rs`
- `crates/tau-slack-runtime/src/slack_runtime/slack_api_client.rs`
- `crates/tau-slack-runtime/src/slack_runtime/tests.rs`

## Risks
- Provider API mismatch for multipart/v2 uploads.
- Side effects on non-send-file Slack run rendering.

## Mitigations
- Keep send-file flow behind explicit directive parser.
- Add regression assertions that regular text runs remain unchanged.
