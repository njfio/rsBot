# Plan: Issue #3032 - `/auth rotate-key` credential-store key rotation

## Approach
1. Add RED tests for parser and runtime execution (success/failure) plus help usage coverage.
2. Extend auth command model/parser with rotate-key variant and usage string.
3. Implement rotate-key execution:
   - validate keyed encryption mode,
   - resolve old-key (flag override or config key),
   - load store with old key,
   - save store with new key,
   - emit deterministic text/JSON summary.
4. Update command catalog usage/details/example.
5. Rerun targeted tests and baseline checks.

## Affected Paths
- `crates/tau-provider/src/auth_commands_runtime.rs`
- `crates/tau-ops/src/command_catalog.rs`
- `crates/tau-coding-agent/src/tests/auth_provider/auth_and_provider.rs`
- `crates/tau-coding-agent/src/tests/auth_provider/commands_and_packages.rs`
- `specs/milestones/m186/index.md`
- `specs/3032/spec.md`
- `specs/3032/plan.md`
- `specs/3032/tasks.md`

## Risks and Mitigations
- Risk: accidental credential-store corruption on rotation failure.
  - Mitigation: load/decrypt fully first, then single atomic save call; fail early on validation.
- Risk: key value leakage in output.
  - Mitigation: never print old/new key values; emit only status/path/counts.
- Risk: ambiguous key source.
  - Mitigation: explicit precedence (`--old-key` over config key) and deterministic error text.

## Interfaces / Contracts
Rotate-key command contract:
- `/auth rotate-key --new-key <key> [--old-key <key>] [--json]`

JSON output contract (success):
- `command`: `auth.rotate_key`
- `status`: `rotated`
- `credential_store`: path
- `provider_entries`: integer
- `integration_entries`: integer

Error contract:
- `auth rotate-key error: ...` text with fail-closed behavior.

## ADR
Not required (command/runtime behavior extension within existing auth boundary).
