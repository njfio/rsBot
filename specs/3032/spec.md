# Spec: Issue #3032 - `/auth rotate-key` command for credential-store key rotation

Status: Implemented

## Problem Statement
Operators can store encrypted provider/integration credentials, but there is no explicit `/auth` command to rotate credential-store encryption keys. This creates operational risk during key hygiene/incident response.

## Acceptance Criteria

### AC-1 `/auth` parser supports rotate-key command
Given `/auth` command input,
When parsing `rotate-key --new-key <key> [--old-key <key>] [--json]`,
Then parser returns a structured rotate-key command variant and enforces usage/duplicate/missing-arg errors.

### AC-2 Runtime rotates credential-store key without data loss
Given an encrypted credential store with provider/integration entries,
When executing `/auth rotate-key --new-key <new> [--old-key <old>]`,
Then store entries remain readable with the new key and are no longer decryptable with the old key.

### AC-3 Command help/usage reflect rotate-key capability
Given command help surfaces,
When rendering `/help commands` or `/help auth`,
Then `/auth` usage includes `rotate-key` and an example.

### AC-4 Failure modes are explicit and fail-closed
Given invalid inputs or non-keyed encryption mode,
When executing rotate-key,
Then command returns deterministic error output and does not partially persist unsafe state.

### AC-5 Baseline checks remain green
Given all updates,
When running checks,
Then targeted tests plus `cargo fmt --check` and `cargo check -q` pass.

## Scope

### In Scope
- `crates/tau-provider/src/auth_commands_runtime.rs`
- `crates/tau-ops/src/command_catalog.rs`
- `crates/tau-coding-agent/src/tests/auth_provider/auth_and_provider.rs`
- `crates/tau-coding-agent/src/tests/auth_provider/commands_and_packages.rs`
- `specs/milestones/m186/index.md`
- `specs/3032/*`

### Out of Scope
- Changing credential-store on-disk schema version.
- Adding external key management providers.
- CLI flag-level key rotation outside `/auth` command flow.

## Conformance Cases
- C-01: parser accepts rotate-key command and captures options.
- C-02: parser rejects missing/duplicate key flags with rotate-key usage.
- C-03: rotate-key execution success preserves entries and re-encrypts with new key.
- C-04: rotate-key execution fails with explicit error in invalid mode/empty key cases.
- C-05: command help includes updated `/auth` usage and rotate-key example.
- C-06: baseline checks pass.

## Success Metrics / Observable Signals
- `cargo test -p tau-coding-agent auth_provider::auth_and_provider::unit_parse_auth_command_supports_login_status_logout_and_json`
- `cargo test -p tau-coding-agent auth_provider::auth_and_provider::functional_execute_auth_command_rotate_key_rotates_store_without_data_loss`
- `cargo test -p tau-coding-agent auth_provider::auth_and_provider::regression_execute_auth_command_rotate_key_fails_closed_for_invalid_inputs`
- `cargo test -p tau-coding-agent auth_provider::commands_and_packages::functional_render_command_help_supports_auth_topic_without_slash`
- `cargo fmt --check`
- `cargo check -q`

## Approval Gate
P1 scope: spec authored/reviewed by agent; implementation proceeds and is flagged for human review in PR.
