# Plan: Issue #3036 - Contributor and Security policy docs hardening

## Approach
1. Extend docs conformance script with assertions for contributor/security docs and README discoverability links.
2. Run conformance script to capture RED failure against current docs/link content.
3. Update `CONTRIBUTING.md`, `SECURITY.md`, and `README.md` to satisfy new assertions and operational expectations.
4. Re-run conformance (GREEN) plus baseline verification commands.
5. Finalize spec/tasks status and prepare PR with AC mapping and RED/GREEN evidence.

## Affected Paths
- `CONTRIBUTING.md`
- `SECURITY.md`
- `README.md`
- `scripts/dev/test-docs-capability-archive.sh`
- `specs/milestones/m187/index.md`
- `specs/3036/spec.md`
- `specs/3036/plan.md`
- `specs/3036/tasks.md`

## Risks and Mitigations
- Risk: Overly verbose policy text reduces usability.
  - Mitigation: Keep sections concise with actionable bullets and explicit commands.
- Risk: Docs drift undetected later.
  - Mitigation: Encode required anchors/links in docs conformance script.

## Interfaces / Contracts
- Root docs contract: `CONTRIBUTING.md` and `SECURITY.md` must remain present and linked from `README.md`.
- Conformance contract: `scripts/dev/test-docs-capability-archive.sh` validates required sections/links.

## ADR
Not required (documentation/process update only).
