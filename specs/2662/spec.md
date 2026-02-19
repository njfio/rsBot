# Spec: Issue #2662 - Discord mention normalization and chunking parity validation

Status: Implemented

## Problem Statement
Discord inbound payloads can contain mention tokens such as `<@123>` and `<@!456>`. Without normalization, channel policies and downstream prompts receive low-signal raw IDs rather than user-friendly display mentions. Additionally, Discord outbound chunking must remain safely capped to 2000 characters.

## Acceptance Criteria

### AC-1 Discord mention tokens normalize to display names when metadata exists
Given a Discord inbound payload with mention metadata,
When live-ingress parsing normalizes the message content,
Then `<@ID>` and `<@!ID>` tokens are replaced with `@DisplayName` using mention metadata.

### AC-2 Unresolved mention tokens fail closed to original content
Given a Discord inbound payload with mention tokens that cannot be resolved,
When parsing runs,
Then unresolved tokens remain unchanged and parsing still succeeds.

### AC-3 Discord outbound chunking remains capped at 2000 chars
Given outbound Discord delivery with payloads longer than 2000 characters,
When dispatch chunking executes,
Then each chunk length is <= 2000 characters.

### AC-4 Scoped verification gates pass
Given this scope,
When scoped formatting, linting, and targeted tests run,
Then all required gates pass with evidence.

## Scope

### In Scope
- Mention normalization in `tau-multi-channel` Discord ingress parsing.
- Targeted tests for normalized/resolution fallback behavior.
- Targeted tests validating Discord chunk cap behavior.
- Roadmap checklist updates for completed G10 items in this scope.

### Out of Scope
- Full Serenity-based Discord runtime rewrite.
- Discord history backfill or streaming placeholder message edits.
- New channel-policy semantics unrelated to mention token normalization.

## Conformance Cases
- C-01 (conformance): Discord mention tokens map to `@DisplayName` for both `<@ID>` and `<@!ID>` forms.
- C-02 (regression): unresolved mention tokens remain unchanged.
- C-03 (functional): outbound Discord chunks cap at 2000 chars regardless of configured larger max chunk.
- C-04 (verify): scoped fmt/clippy/tests pass.

## Success Metrics / Observable Signals
- Inbound Discord text contains normalized display mentions where metadata is available.
- No parser regressions for unresolved or partial mention metadata.
- Outbound Discord chunking remains safety-capped at 2000 chars.
