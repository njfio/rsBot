# Plan: Issue #3016 - Contributor/security docs publish

## Approach
1. Add a shell conformance script that checks file presence and required section markers for `CONTRIBUTING.md` and `SECURITY.md`.
2. Run script before docs exist to capture RED failure.
3. Create both root docs with concise, repo-appropriate policy content.
4. Re-run conformance script and baseline checks for GREEN/regression evidence.

## Affected Paths
- `CONTRIBUTING.md`
- `SECURITY.md`
- `scripts/dev/test-contributor-security-docs.sh`
- `specs/milestones/m182/index.md`
- `specs/3016/spec.md`
- `specs/3016/plan.md`
- `specs/3016/tasks.md`

## Risks and Mitigations
- Risk: over-generalized policy text.
  - Mitigation: keep concise and tied to existing Tau workflows/commands.
- Risk: section-marker drift.
  - Mitigation: deterministic script assertions for required headings.

## ADR
Not required (documentation-only scope).
