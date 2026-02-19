# Spec: Issue #2613 - Encrypted gateway auth secret migration

Status: Implemented

## Problem Statement
`gateway-openresponses` auth secrets (`--gateway-openresponses-auth-token` and `--gateway-openresponses-auth-password`) are still consumed directly as plaintext CLI/env values. This leaves an unencrypted secret surface outside the credential-store workflow and prevents operator rotation by credential id.

## Acceptance Criteria

### AC-1 Gateway auth secret IDs are supported by CLI parsing and validation
Given gateway openresponses auth configuration,
When operators provide `--gateway-openresponses-auth-token-id` or `--gateway-openresponses-auth-password-id`,
Then validation treats those IDs as satisfying auth requirements equivalently to direct secrets.

### AC-2 Gateway auth secret resolution uses encrypted credential-store entries with fail-closed semantics
Given gateway openresponses runtime configuration,
When auth secret IDs are provided,
Then startup resolves secrets via encrypted credential store and fails closed on missing, revoked, or empty entries.

### AC-3 Remote-profile diagnostics and operator docs reflect secret-id migration path
Given gateway remote profile planning/inspection and docs,
When token/password IDs are configured,
Then readiness checks and guidance report auth secrets as configured and docs include secure migration/rotation workflow via `/integration-auth` IDs.

### AC-4 Scoped verification gates are green
Given the migration changes,
When scoped checks run,
Then formatting, linting, and targeted test suites pass.

## Scope

### In Scope
- Add `gateway-openresponses` auth token/password credential-store ID flags.
- Resolve gateway auth secrets via credential-store ID helper with fail-closed errors.
- Update gateway remote profile checks to treat ID-backed secrets as configured.
- Update gateway operator docs to prefer encrypted secret-id workflow.

### Out of Scope
- Full provider API-key migration into credential-store IDs.
- New secret-store backends or encryption algorithm changes.
- Multi-channel connector secret model redesign.

## Conformance Cases
- C-01 (unit): CLI validation accepts token/password ID flags as auth requirements.
- C-02 (functional): gateway auth resolver trims direct secrets and resolves secret IDs via credential store.
- C-03 (regression): gateway auth resolver fails closed on missing/revoked/empty credential-store IDs.
- C-04 (integration): gateway openresponses config builder and remote profile checks treat ID-backed secrets as configured.
- C-05 (verify): `cargo fmt --check`, `cargo clippy -p tau-onboarding -p tau-cli -- -D warnings`, `cargo test -p tau-onboarding gateway_openresponses`, and `cargo test -p tau-cli gateway_remote_profile` pass.

## Success Metrics / Observable Signals
- Gateway server can be launched with auth mode token/password-session using `*-id` flags without plaintext secrets.
- Invalid/revoked IDs fail startup deterministically with actionable error text.
- Remote profile inspect/plan no longer reports false missing-secret findings when ID-backed secrets are used.
