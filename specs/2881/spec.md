# Spec: Issue #2881 - chat multi-line input contracts

Status: Implemented

## Problem Statement
Tau Ops chat supports message send and session workflows, but does not expose deterministic multiline compose contracts nor explicit newline-preservation validation for submitted chat content. This leaves the PRD checklist item “Multi-line input works (Shift+Enter)” unverifiable.

## Scope
In scope:
- Add deterministic multiline compose contract markers in the chat form on `/ops/chat`.
- Validate gateway send flow preserves embedded newlines in submitted message content.
- Validate non-chat routes preserve hidden chat panel contracts after multiline send.

Out of scope:
- Client-side keybinding JavaScript runtime changes.
- Rich text editing features.
- New dependencies.

## Acceptance Criteria
### AC-1 Chat route exposes deterministic multiline compose form contracts
Given the `/ops/chat` shell,
when UI markup renders,
then chat compose textarea exposes deterministic multiline markers including Shift+Enter hint contract tokens.

### AC-2 Gateway send preserves multiline content contract
Given multiline chat input content containing embedded `\n`,
when `POST /ops/chat/send` is called,
then gateway appends message content preserving embedded newlines in session storage.

### AC-3 Chat route renders multiline session content contracts
Given a session containing multiline message content,
when `/ops/chat` renders,
then chat transcript contracts surface the multiline content payload for the message row.

### AC-4 Non-chat routes preserve hidden panel contracts after multiline send
Given multiline content submitted for a target session,
when `/ops` and `/ops/sessions` are rendered with that session query,
then chat panel stays hidden while preserving active session key contract markers.

### AC-5 Regression safety for prior chat contract phases
Given existing chat contract suites,
when `spec_2830`, `spec_2834`, `spec_2858`, `spec_2862`, `spec_2866`, `spec_2870`, and `spec_2872` rerun,
then all suites remain green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | `/ops/chat` shell render | inspect compose markup | multiline compose markers + Shift+Enter tokens present |
| C-02 | AC-2 | Integration | multiline payload with embedded newline | `POST /ops/chat/send` | session store contains preserved newline content |
| C-03 | AC-3 | Functional | session with multiline message | render `/ops/chat` | transcript row includes multiline content payload |
| C-04 | AC-4 | Integration | session updated by multiline send | render `/ops` + `/ops/sessions` | hidden chat panel markers preserved with active session |
| C-05 | AC-5 | Regression | prior chat suites | rerun suites | no regressions |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_2881 -- --test-threads=1` passes.
- `cargo test -p tau-gateway 'spec_2881' -- --test-threads=1` passes.
- Required chat regression suites remain green.
