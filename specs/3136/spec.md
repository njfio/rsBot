# Spec: Issue #3136 - readme capability sync

Status: Accepted

## Problem Statement
Root `README.md` under-describes merged ops dashboard coverage and currently frames dashboard UI as webchat-only, which is no longer precise.

## Scope
In scope:
- Update README capability bullets to include merged `/ops/tools-jobs` and `/ops/channels` contract surfaces.
- Clarify dashboard status language to match current Leptos SSR ops shell behavior.

Out of scope:
- Runtime code changes.
- API behavior changes.
- New docs guides.

## Acceptance Criteria
### AC-1 README capability list includes current ops dashboard surfaces
Given merged ops dashboard slices for tools/jobs and channels,
when README is read,
then it explicitly states these operator surfaces as current capabilities.

### AC-2 README dashboard status language is precise
Given the dashboard is no longer only webchat shell,
when README capability status is read,
then language references route-based Leptos SSR ops shell and current boundaries.

### AC-3 README remains internally consistent with operator docs links
Given operator deployment and API reference docs exist,
when README is read,
then links and summary text stay consistent and non-contradictory.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Conformance | README pre-change lacks explicit ops dashboard coverage | inspect README | new capability bullet references `/ops/tools-jobs` and `/ops/channels` |
| C-02 | AC-2 | Conformance | dashboard status text is webchat-only | inspect README | status text references Leptos SSR ops shell + boundary |
| C-03 | AC-3 | Regression | operator docs links already present | inspect README | links remain present and unchanged/valid |

## Success Metrics / Signals
- `rg -n "/ops/tools-jobs|/ops/channels|Leptos SSR ops shell" README.md`
- `rg -n "operator-deployment-guide|gateway-api-reference" README.md`
