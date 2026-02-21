# Spec: Issue #3024 - Gateway API route inventory drift guard

Status: Implemented

## Problem Statement
The gateway API reference contains explicit route-count markers (`70` route calls and `78` method-path entries), but no deterministic automation currently enforces that these markers remain synchronized with router reality. Drift here can mislead operators and reviewers.

## Acceptance Criteria

### AC-1 Deterministic route inventory script exists
Given repository source and gateway API docs,
When running the inventory script,
Then it outputs deterministic JSON/Markdown artifacts with route call count, method-path row count, marker values, and drift verdict.

### AC-2 Docs marker drift is validated
Given `docs/guides/gateway-api-reference.md` markers,
When marker values do not match extracted counts,
Then the script exits non-zero with explicit mismatch diagnostics.

### AC-3 Conformance test coverage exists
Given the script contract,
When running conformance tests,
Then tests cover success and mismatch failure behavior.

### AC-4 Docs command contract is discoverable
Given the gateway API reference,
When reading operator verification guidance,
Then it includes the deterministic inventory/drift check command.

### AC-5 Baseline checks remain green
Given all updates,
When running baseline checks,
Then `cargo fmt --check` and `cargo check -q` pass.

## Scope

### In Scope
- `scripts/dev/gateway-api-route-inventory.sh` (new)
- `scripts/dev/test-gateway-api-route-inventory.sh` (new)
- `scripts/dev/test-docs-capability-archive.sh` (update)
- `docs/guides/gateway-api-reference.md` (update command contract)
- `tasks/reports/gateway-api-route-inventory.json` (generated)
- `tasks/reports/gateway-api-route-inventory.md` (generated)
- `specs/milestones/m184/index.md`
- `specs/3024/*`

### Out of Scope
- Runtime gateway route behavior changes.
- CI workflow rewiring beyond script/test additions.
- Auth/policy semantics changes.

## Conformance Cases
- C-01: Inventory script succeeds against current repo and emits JSON+Markdown with expected schema keys.
- C-02: Inventory script fails when docs marker values are intentionally mismatched (fixture/test mode).
- C-03: Docs capability test enforces inventory command presence in API reference.
- C-04: Baseline checks pass.

## Success Metrics / Observable Signals
- `bash scripts/dev/test-gateway-api-route-inventory.sh`
- `bash scripts/dev/test-docs-capability-archive.sh`
- `scripts/dev/gateway-api-route-inventory.sh --output-json tasks/reports/gateway-api-route-inventory.json --output-md tasks/reports/gateway-api-route-inventory.md`
- `cargo fmt --check`
- `cargo check -q`

## Approval Gate
P1 scope: spec authored/reviewed by agent; implementation proceeds and is flagged for human review in PR.
