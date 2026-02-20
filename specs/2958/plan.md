# Plan: Issue #2958 - Operator deployment guide and live validation

## Approach
1. Audit existing runbooks (`gateway-ops`, `deployment-ops`, `ops-readiness-live-validation`) to avoid duplication.
2. Write a focused operator deployment guide that stitches these into one linear procedure:
   prerequisites -> credentials -> launch -> auth -> readiness checks -> rollback.
3. Link the guide in `docs/README.md` audience index.
4. Run live validation commands directly from the documented flow and record outputs in PR evidence.

## Affected Paths
- `docs/guides/operator-deployment-guide.md` (new)
- `docs/README.md` (index link)

## Risks and Mitigations
- Risk: command drift from runtime flags.
  - Mitigation: execute documented commands in local environment; keep commands copy/paste exact.
- Risk: duplicate or conflicting guidance with existing runbooks.
  - Mitigation: use this guide as operator entrypoint and deep-link to specialized runbooks.

## Interfaces / Contracts
- Gateway OpenResponses CLI flags in `tau-coding-agent`
- HTTP endpoints: `/gateway/status`, `/cortex/status`, `/webchat`
- `scripts/dev/operator-readiness-live-check.sh` fail-closed readiness contract

## ADR
Not required (docs-only change, no architecture/protocol/dependency change).
