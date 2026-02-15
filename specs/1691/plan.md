# Issue 1691 Plan

Status: Reviewed

## Approach

1. Patch module headers (`//!`) in onboarding startup/transport/profile files.
2. Focus each header on:
   - startup phase contract (preflight -> resolution -> dispatch)
   - wizard/profile persistence invariants
   - transport mode diagnostics/failure expectations
3. Run targeted onboarding crate tests plus docs-link regression check.

## Affected Areas

- `crates/tau-onboarding/src/startup_*.rs`
- `crates/tau-onboarding/src/onboarding_*.rs`
- `crates/tau-onboarding/src/profile_*.rs`
- `specs/1691/*`

## Risks And Mitigations

- Risk: module docs become generic boilerplate
  - Mitigation: keep docs phase/contract-oriented and file-specific.
- Risk: broad file churn causes merge friction
  - Mitigation: header-only doc additions, no logic edits.

## ADR

No architecture/dependency/protocol change. ADR not required.
