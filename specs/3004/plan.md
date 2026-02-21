# Plan: Issue #3004 - Tau gaps revalidation refresh

## Approach
1. Capture authoritative current-state evidence:
   - GitHub issue states for follow-up IDs.
   - Repository anchors for OpenTelemetry export, provider token-bucket limits, graph endpoint, multiprocess types, and external coding-agent bridge.
   - Snapshot metrics (HEAD, milestone/spec/package counts).
2. Add script-level conformance checks that encode expected refreshed wording and status rows.
3. Run script before doc update to capture RED evidence.
4. Update `tasks/tau-gaps-issues-improvements.md` to match current evidence while preserving evidence-first structure.
5. Re-run conformance test and baseline gates for GREEN/regression evidence.

## Affected Paths
- `tasks/tau-gaps-issues-improvements.md`
- `scripts/dev/test-tau-gaps-issues-improvements.sh`
- `specs/milestones/m179/index.md`
- `specs/3004/spec.md`
- `specs/3004/plan.md`
- `specs/3004/tasks.md`

## Risks and Mitigations
- Risk: overfitting doc to stale assumptions.
  - Mitigation: only keep claims backed by current `gh issue` and code-search evidence.
- Risk: brittle conformance assertions.
  - Mitigation: assert key contract markers/status rows rather than entire document text.
- Risk: metadata drift.
  - Mitigation: source values from deterministic commands and include command evidence in PR.

## Interfaces / Contracts
- Documentation contract: evidence-first roadmap table and follow-up section remain present.
- Test contract: `scripts/dev/test-tau-gaps-issues-improvements.sh` fails when stale markers return.

## ADR
No new architecture decision; ADR not required.
