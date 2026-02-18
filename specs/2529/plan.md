# Plan #2529

## Approach
1. Add RED tests for Discord send-file delivery in multi-channel outbound.
2. Implement Discord send-file provider dispatch.
3. Add RED tests for Slack send-file directive extraction + upload dispatch.
4. Implement Slack directive parsing and v2 upload flow in runtime/API client.
5. Re-run regression slices for existing outbound pathways.

## Risks
- Multipart upload handling differences across providers.
- Runtime response rendering regressions for Slack text paths.

## Mitigations
- Keep Slack send-file handling explicit and isolated from normal text response rendering.
- Add regressions for non-send-file runs.
