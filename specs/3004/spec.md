# Spec: Issue #3004 - Refresh Tau gaps revalidation doc to current closure state

Status: Reviewed

## Problem Statement
`tasks/tau-gaps-issues-improvements.md` includes stale closure states and outdated metadata. It must be refreshed so roadmap statuses and follow-up issue references match current GitHub issue state and repository evidence.

## Acceptance Criteria

### AC-1 Roadmap status rows reflect current closure evidence
Given the roadmap closure table in `tasks/tau-gaps-issues-improvements.md`,
When reviewing rows tied to M104 follow-up outcomes,
Then statuses for stale branches, provider token-bucket rate limiting, OpenTelemetry export, graph visualization, multiprocess architecture, and external coding agent bridge reflect current delivered state with evidence notes.

### AC-2 Follow-up issue section no longer reports closed items as open
Given the follow-up issue section in the same document,
When reviewing issues `#2608`, `#2609`, `#2610`, `#2611`, `#2612`, `#2613`, `#2616`, `#2617`, `#2618`, `#2619`,
Then each is listed as `Closed` (or otherwise accurately represented) and no "open follow-up items" wording remains for that set.

### AC-3 Metadata snapshot is refreshed to current repo state
Given document header metrics,
When comparing to current repository state,
Then date, HEAD, milestone/spec/package counts, and other snapshot details are updated and consistent with deterministic commands.

### AC-4 Conformance checks fail on stale content and pass after refresh
Given a script-level conformance test,
When run before doc refresh,
Then it fails on stale content;
And when run after doc refresh,
Then it passes.

## Scope

### In Scope
- `tasks/tau-gaps-issues-improvements.md` status/content refresh.
- `specs/milestones/m179/index.md` and `specs/3004/*` lifecycle artifacts.
- A deterministic script-level conformance test for the refreshed doc.

### Out of Scope
- Implementing new runtime features.
- Rewriting all prior roadmap decisions.
- Changing issue hierarchy/labels beyond normal status progression.

## Conformance Cases
- C-01: closure table rows show current statuses for roadmap items tied to M104 follow-up issues.
- C-02: follow-up issue section marks `#2608/#2609/#2610/#2611/#2612/#2613/#2616/#2617/#2618/#2619` as closed.
- C-03: document snapshot metadata matches current repo measurements.
- C-04: `scripts/dev/test-tau-gaps-issues-improvements.sh` passes.

## Success Metrics / Observable Signals
- `bash scripts/dev/test-tau-gaps-issues-improvements.sh` passes.
- `cargo fmt --check` passes.
- `cargo check -q` passes.

## Approval Gate
P2 scope: agent-authored spec, self-reviewed, implementation proceeds with human review in PR.
