# Spec: Issue #2870 - chat markdown and code-block contracts

Status: Implemented

## Problem Statement
Tau Ops chat transcript currently renders message content as plain text rows and lacks deterministic SSR markers for markdown and code-block rendering contracts. This prevents conformance validation for PRD chat expectations around markdown and code presentation.

## Scope
In scope:
- Add deterministic markdown-render marker contracts for assistant chat rows.
- Add deterministic code-block marker contracts including language and code payload attributes.
- Preserve route-safe behavior on `/ops`, `/ops/chat`, and `/ops/sessions`.

Out of scope:
- Full client-side syntax-highlighting engine.
- New chat transport, persistence, or API changes.
- New dependencies.

## Acceptance Criteria
### AC-1 Markdown rows expose render markers
Given assistant chat content containing markdown syntax,
when chat transcript SSR markup is rendered,
then markdown contract marker attributes are present for that row.

### AC-2 Code-block rows expose language and payload markers
Given assistant chat content containing a fenced code block,
when chat transcript SSR markup is rendered,
then code-block marker attributes include deterministic language and code payload values.

### AC-3 `/ops/chat` route renders markdown and code markers in visible chat panel
Given `/ops/chat` request with markdown+code content in the active session,
when shell markup renders,
then visible chat panel includes markdown and code marker contracts.

### AC-4 Non-chat routes preserve hidden-panel markdown and code markers
Given `/ops` and `/ops/sessions` requests with markdown+code content in the active session,
when shell markup renders,
then chat panel remains hidden and markdown/code markers remain present.

### AC-5 Regression safety for existing chat contracts
Given existing chat contract suites,
when `spec_2830`, `spec_2858`, `spec_2862`, and `spec_2866` rerun,
then all existing contracts remain green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | assistant row contains markdown (header/list/link/table text) | render UI shell | row exposes `tau-ops-chat-markdown-*` marker |
| C-02 | AC-2 | Functional | assistant row contains fenced code block | render UI shell | row exposes `tau-ops-chat-code-block-*` marker with `data-language` and `data-code` |
| C-03 | AC-3 | Integration | gateway `/ops/chat` request with markdown+code message | render response | visible chat panel exposes markdown and code markers |
| C-04 | AC-4 | Integration | gateway `/ops` and `/ops/sessions` requests with markdown+code message | render response | hidden chat panel exposes markdown and code markers |
| C-05 | AC-5 | Regression | existing chat suites | rerun suites | no regression |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_2870 -- --test-threads=1` passes.
- `cargo test -p tau-gateway 'spec_2870' -- --test-threads=1` passes.
- `spec_2830`, `spec_2858`, `spec_2862`, and `spec_2866` remain green.
