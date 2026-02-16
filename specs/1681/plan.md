# Issue 1681 Plan

Status: Reviewed

## Approach

1. Tests-first:
   - run `scripts/dev/test-package-manifest-domain-split.sh` before it exists to
     capture RED.
2. Implement split:
   - create `crates/tau-skills/src/package_manifest/schema.rs` for manifest
     schema models/constants
   - create `crates/tau-skills/src/package_manifest/validation.rs` for
     component/path/url/checksum validation helpers
   - create `crates/tau-skills/src/package_manifest/io.rs` for local/remote
     component loading helpers
   - wire `package_manifest.rs` with `mod schema; mod validation; mod io;`
3. Add conformance harness:
   - `scripts/dev/test-package-manifest-domain-split.sh` verifies line budget
     and module/file markers.
4. Run behavior/quality verification:
   - `cargo test -p tau-skills package_manifest`
   - `scripts/dev/roadmap-status-sync.sh --check --quiet`
   - `cargo fmt --check`
   - `cargo clippy -p tau-skills -- -D warnings`

## Affected Areas

- `crates/tau-skills/src/package_manifest.rs`
- `crates/tau-skills/src/package_manifest/schema.rs`
- `crates/tau-skills/src/package_manifest/validation.rs`
- `crates/tau-skills/src/package_manifest/io.rs`
- `scripts/dev/test-package-manifest-domain-split.sh`
- `specs/1681/spec.md`
- `specs/1681/plan.md`
- `specs/1681/tasks.md`

## Risks And Mitigations

- Risk: helper extraction drifts error messages/contracts.
  - Mitigation: move code token-for-token and run targeted package-manifest
    tests.
- Risk: module imports create private visibility breakage.
  - Mitigation: use `pub(super)` visibility for extracted internals and compile
    with package tests.

## Interfaces / Contracts

- No public CLI/package behavior changes.
- Existing package-manifest validation and install/update flows remain intact.
- Split boundaries are asserted via conformance harness markers.

## ADR

No dependency/protocol/architecture decision changes; ADR not required.
