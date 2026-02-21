# Plan: Issue #3020 - Capability docs and archive workflow refresh

## Approach
1. Add conformance tests first:
   - docs capability markers + API inventory signal test.
   - implemented-spec archive script fixture test.
2. Run tests before implementation for RED evidence.
3. Implement docs updates:
   - README positioning and capability map.
   - operator guide hardening.
   - API reference inventory summary and validation procedure updates.
4. Implement archival workflow:
   - `spec-archive-index.sh` emits JSON/Markdown summary of implemented specs.
   - `spec-branch-archive-ops.md` links branch prune workflow.
5. Re-run conformance + baseline checks for GREEN/regression evidence.

## Affected Paths
- `README.md`
- `docs/guides/operator-deployment-guide.md`
- `docs/guides/gateway-api-reference.md`
- `docs/guides/spec-branch-archive-ops.md` (new)
- `scripts/dev/spec-archive-index.sh` (new)
- `scripts/dev/test-spec-archive-index.sh` (new)
- `scripts/dev/test-docs-capability-archive.sh` (new)
- `specs/milestones/m183/index.md`
- `specs/3020/spec.md`
- `specs/3020/plan.md`
- `specs/3020/tasks.md`

## Risks and Mitigations
- Risk: route count drift.
  - Mitigation: include explicit verification command and conformance marker assertions.
- Risk: archive script overfits local filesystem.
  - Mitigation: support `--spec-root` and deterministic outputs for fixture tests.
- Risk: docs become verbose/noisy.
  - Mitigation: concise operational sections with direct commands and links.

## Interfaces / Contracts
- Docs contracts via marker-based conformance checks.
- Archive script contract:
  - deterministic schema (`schema_version=1`) JSON output,
  - markdown summary with implemented-spec counts,
  - default output location under `tasks/reports/`.

## ADR
Not required (docs/script workflow only).
