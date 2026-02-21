# Spec: Issue #3020 - Refresh README/operator/API docs and add archive workflow

Status: Implemented

## Problem Statement
Current docs need tighter capability truth and discoverability, and operators requested explicit API coverage visibility (70+ gateway routes). Completed spec and merged-branch archival workflows also need deterministic automation.

## Acceptance Criteria

### AC-1 README clearly states what Tau is and current capabilities
Given `README.md`,
When read by a new operator/contributor,
Then it clearly explains what Tau is, what it does today, and links operator/API docs.

### AC-2 Operator deployment guide is explicit and current
Given `docs/guides/operator-deployment-guide.md`,
When followed,
Then startup, auth, readiness, troubleshooting, and rollback procedures are complete and internally consistent.

### AC-3 API reference explicitly documents 70+ gateway routes
Given `docs/guides/gateway-api-reference.md` and gateway router source,
When validating endpoint inventory,
Then docs present coverage guidance and explicitly state route inventory at or above 70 routes.

### AC-4 Implemented-spec archival report workflow exists
Given repository specs,
When running archival script,
Then it emits deterministic JSON/Markdown artifacts listing implemented specs and summary counts.

### AC-5 Branch archival workflow is linked and validated
Given branch archival operations,
When following docs/workflow,
Then deterministic branch prune/report flow is discoverable and test-enforced.

### AC-6 Conformance and baseline checks pass
Given all updates,
When running conformance and baseline commands,
Then checks pass.

## Scope

### In Scope
- `README.md`
- `docs/guides/operator-deployment-guide.md`
- `docs/guides/gateway-api-reference.md`
- `scripts/dev/spec-archive-index.sh` (new)
- `scripts/dev/test-spec-archive-index.sh` (new)
- `scripts/dev/test-docs-capability-archive.sh` (new)
- `docs/guides/spec-branch-archive-ops.md` (new)
- `specs/milestones/m183/index.md`
- `specs/3020/*`

### Out of Scope
- Runtime endpoint behavior changes.
- CI workflow restructuring.
- Dependency upgrades.

## Conformance Cases
- C-01: README contains capability truth markers + docs links.
- C-02: operator guide contains required runbook markers (startup/auth/readiness/rollback).
- C-03: API reference includes explicit 70+ route inventory signal and route-coverage validation instructions.
- C-04: `scripts/dev/spec-archive-index.sh` emits valid JSON/Markdown archive artifacts.
- C-05: docs/archive conformance script passes.
- C-06: `cargo fmt --check` and `cargo check -q` pass.

## Success Metrics / Observable Signals
- `bash scripts/dev/test-spec-archive-index.sh`
- `bash scripts/dev/test-docs-capability-archive.sh`
- `cargo fmt --check`
- `cargo check -q`

## Approval Gate
P1 scope: spec authored/reviewed by agent; implementation proceeds and is flagged for human review in PR.
