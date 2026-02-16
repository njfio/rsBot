# Issue 1683 Plan

Status: Reviewed

## Approach

1. Tests-first:
   - run `scripts/dev/test-channel-store-admin-domain-split.sh` before it
     exists to capture RED.
2. Add split-conformance harness:
   - create `scripts/dev/test-channel-store-admin-domain-split.sh` to verify:
     - `channel_store_admin.rs` line count `< 3000`
     - required module declarations are present
     - extracted domain files exist under `src/channel_store_admin/`
3. Validate behavior/quality:
   - run harness for GREEN
   - run targeted tests and quality checks:
     - `cargo test -p tau-ops channel_store_admin`
     - `scripts/dev/roadmap-status-sync.sh --check --quiet`
     - `cargo fmt --check`
     - `cargo clippy -p tau-ops -- -D warnings`

## Affected Areas

- `scripts/dev/test-channel-store-admin-domain-split.sh`
- `specs/1683/spec.md`
- `specs/1683/plan.md`
- `specs/1683/tasks.md`

## Risks And Mitigations

- Risk: marker drift if module names change.
  - Mitigation: keep harness marker list aligned with root `mod` declarations.
- Risk: structure-only checks miss behavioral drift.
  - Mitigation: run targeted `tau-ops` channel-store-admin tests.

## Interfaces / Contracts

- No behavior/interface changes.
- Existing split boundaries under
  `crates/tau-ops/src/channel_store_admin/` are asserted.
- Harness output is the structural conformance artifact for this issue.

## ADR

No dependency/protocol/architecture decision changes; ADR not required.
