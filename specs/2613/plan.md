# Plan: Issue #2613 - Encrypted gateway auth secret migration

## Approach
1. Add red tests for gateway validation and auth resolution using secret IDs.
2. Add red integration tests for remote profile/config builder behavior under ID-backed auth.
3. Add new CLI args for token/password secret IDs.
4. Route gateway auth resolution through `resolve_secret_from_cli_or_store_id` and propagate errors.
5. Update gateway remote profile auth-configured checks to include secret IDs.
6. Update gateway docs with secure migration/rotation steps using `/integration-auth` IDs.
7. Run scoped verify gates and record AC mapping evidence.

## Affected Modules
- `crates/tau-cli/src/cli_args.rs`
- `crates/tau-cli/src/validation.rs`
- `crates/tau-cli/src/gateway_remote_profile.rs`
- `crates/tau-onboarding/src/startup_transport_modes.rs`
- `crates/tau-onboarding/src/startup_transport_modes/tests.rs`
- `docs/guides/gateway-ops.md`
- `docs/guides/gateway-remote-access.md`
- `specs/2613/spec.md`
- `specs/2613/plan.md`
- `specs/2613/tasks.md`

## Risks / Mitigations
- Risk: auth startup regressions if config builder signature changes.
  - Mitigation: add focused integration tests for gateway config builder and disabled-run paths.
- Risk: validation mismatch between direct secret and secret-id modes.
  - Mitigation: add explicit validation tests for token/password mode with both direct and ID inputs.
- Risk: operator confusion during migration.
  - Mitigation: update gateway docs with side-by-side direct vs secret-id examples and rotation commands.

## Interfaces / Contracts
- New flags:
  - `--gateway-openresponses-auth-token-id`
  - `--gateway-openresponses-auth-password-id`
- Runtime resolution:
  - `resolve_gateway_openresponses_auth` returns `Result<(Option<String>, Option<String>)>` and fails closed when explicit IDs are invalid.
- Validation:
  - Token/password auth mode requirements satisfied by direct secrets OR corresponding secret IDs.

## ADR
- Not required: no new dependencies or protocol schema changes.
