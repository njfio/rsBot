# Plan: Issue #2923 - split tau-dashboard-ui lib below oversized-file threshold

1. Extract the `#[cfg(test)] mod tests` block from `crates/tau-dashboard-ui/src/lib.rs` into `crates/tau-dashboard-ui/src/tests.rs` and replace with `#[cfg(test)] mod tests;`.
2. Remove the temporary UI exemption entry from `tasks/policies/oversized-file-exemptions.json`.
3. Verify policy and behavior contracts:
   - line count check (`wc -l`)
   - oversized-file guard script
   - scoped tests for `spec_2921` in `tau-dashboard-ui` and `tau-gateway`
   - regression slices for prior memory/session specs.

## Risks / Mitigations
- Risk: test module relocation breaks visibility/imports.
  - Mitigation: keep module nested under crate root (`mod tests;`) and preserve `use super::{...}` imports.
- Risk: accidental behavior changes during refactor.
  - Mitigation: no production logic edits; verify with existing conformance/regression suites.

## Interface / Contract Notes
- No API, trait, protocol, or wire-format changes.
- Pure internal source-layout/test-module refactor + policy metadata cleanup.
