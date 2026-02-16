# Issue 1632 Plan

Status: Reviewed

## Approach

1. Tests-first: extend `.github/scripts/runbook_ownership_docs_check.py` to require ownership tokens for:
   - `docs/guides/dashboard-ops.md`
   - `docs/guides/custom-command-ops.md`
   and map-token references in `docs/guides/runbook-ownership-map.md`.
2. Run check for RED (expected missing-token failures).
3. Update docs:
   - add `## Ownership` sections to dashboard/custom-command runbooks
   - add dashboard/custom-command rows to ownership map table
4. Re-run check for GREEN.
5. Run scoped verification:
   - `scripts/dev/roadmap-status-sync.sh --check --quiet`
   - `cargo fmt --check`
   - `cargo clippy -p tau-onboarding -- -D warnings`

## Affected Areas

- `.github/scripts/runbook_ownership_docs_check.py`
- `docs/guides/dashboard-ops.md`
- `docs/guides/custom-command-ops.md`
- `docs/guides/runbook-ownership-map.md`
- `specs/1632/spec.md`
- `specs/1632/plan.md`
- `specs/1632/tasks.md`

## Risks And Mitigations

- Risk: docs checker becomes brittle from over-specific tokens.
  - Mitigation: enforce stable ownership tokens (section header, crate paths, map link) only.
- Risk: ownership map rows drift from runbook sections.
  - Mitigation: checker validates both runbook tokens and map references.

## ADR

No dependency/protocol/architecture decision changes; ADR not required.
