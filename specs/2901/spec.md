# Spec: Issue #2901 - ops chat assistant token-stream rendering contracts

Status: Implemented

## Problem Statement
The PRD requires agent responses to stream token-by-token, but current `/ops/chat` SSR contracts only expose full assistant message content without token-level rendering metadata. This leaves the chat checklist item under-specified and unverified.

## Scope
In scope:
- Add deterministic chat transcript contracts for assistant token stream metadata and ordered token rows.
- Add UI and gateway conformance tests proving token row count and ordering for assistant responses.
- Preserve existing chat/session/dashboard contract behavior.

Out of scope:
- Provider runtime streaming protocol changes.
- New external dependencies.
- Route/schema changes outside existing `/ops/chat` contracts.

## Acceptance Criteria
### AC-1 Assistant rows expose token-stream metadata
Given `/ops/chat` renders assistant transcript rows,
when an assistant message has non-empty content,
then the row exposes deterministic token-stream metadata markers including token count.

### AC-2 Assistant token rows preserve normalized order
Given assistant message content with multiple tokens,
when chat transcript renders,
then token rows render in deterministic sequence order and match normalized token count.

### AC-3 Non-assistant rows do not emit assistant token-stream markers
Given user/system/tool transcript rows,
when chat transcript renders,
then assistant-specific token-stream marker contracts are absent for non-assistant rows.

### AC-4 Existing chat contracts remain intact
Given existing chat contracts for send/new-session/selector/token-counter/tool-cards/markdown/multiline,
when token-stream contracts are added,
then existing suites remain green unchanged.

### AC-5 Regression safety for sessions/detail contracts
Given existing sessions/detail contracts,
when token-stream contracts are added,
then suites `spec_2838`, `spec_2842`, `spec_2846`, `spec_2885`, `spec_2889`, `spec_2893`, and `spec_2897` remain green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | assistant row present in transcript | render `/ops/chat` | assistant row includes token-stream metadata markers and token count |
| C-02 | AC-2 | Integration | persisted assistant message with multiple tokens | render `/ops/chat` | token rows appear in order with expected normalized count |
| C-03 | AC-3 | Functional | mixed transcript roles | render `/ops/chat` | non-assistant rows omit assistant token-stream marker contracts |
| C-04 | AC-4 | Regression | existing chat contracts | rerun chat suites | existing chat contracts pass unchanged |
| C-05 | AC-5 | Regression | existing sessions/detail contracts | rerun sessions/detail suites | no regressions |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_2901 -- --test-threads=1` passes.
- `cargo test -p tau-gateway spec_2901 -- --test-threads=1` passes.
- Required regression suites remain green.
