# Spec: Issue #2889 - session reset confirmation and clear-session contracts

Status: Implemented

## Problem Statement
Tau Ops sessions surfaces provide session detail, lineage, and branch contracts, but they do not expose deterministic reset-confirmation form contracts tied to an ops reset action. This leaves the PRD checklist item “Reset clears session with confirmation” unverifiable.

## Scope
In scope:
- Add deterministic session reset confirmation form markers in sessions detail view.
- Add gateway ops reset action handling on session detail route contracts.
- Validate reset clears only the target session and preserves route state contracts.
- Validate post-reset session detail renders deterministic empty-state contracts.

Out of scope:
- Gateway API `/gateway/sessions/{session_key}/reset` behavior changes.
- Session branch or merge behavior changes.
- New dependencies.

## Acceptance Criteria
### AC-1 Sessions detail exposes deterministic reset confirmation contracts
Given `/ops/sessions/{session_key}` renders a session detail panel,
when SSR markup is inspected,
then it contains a deterministic reset form contract with confirmation markers, session key markers, and theme/sidebar hidden-state markers.

### AC-2 Reset action clears selected session and redirects deterministically
Given an existing session with timeline entries,
when reset form is posted with confirmation to session detail route,
then gateway clears the selected session and returns a `303` redirect to `/ops/sessions/{session_key}` preserving theme/sidebar query contracts.

### AC-3 Post-reset detail route renders empty-state contracts
Given a successfully reset session,
when redirected detail route renders,
then session detail panel shows empty timeline contracts and validation summary contracts for an empty session.

### AC-4 Reset isolation keeps other sessions unaffected
Given multiple sessions exist,
when one session is reset,
then non-target sessions remain intact and continue rendering their transcript/timeline content.

### AC-5 Regression safety for prior chat/session contract phases
Given existing suites for `spec_2830`, `spec_2834`, `spec_2838`, `spec_2842`, `spec_2846`, `spec_2872`, `spec_2881`, and `spec_2885`,
when rerun,
then all suites remain green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | sessions detail render | inspect SSR markup | reset form + confirmation + hidden state markers present |
| C-02 | AC-2 | Integration | existing session with messages | post reset form | target session cleared + `303` redirect preserves theme/sidebar/session |
| C-03 | AC-3 | Functional | reset session detail route | render detail | empty timeline + clean validation contracts render |
| C-04 | AC-4 | Integration | target + non-target sessions | reset target | non-target session content remains present |
| C-05 | AC-5 | Regression | prior suites | rerun suites | no regressions |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_2889 -- --test-threads=1` passes.
- `cargo test -p tau-gateway 'spec_2889' -- --test-threads=1` passes.
- Required chat/session regression suites remain green.
