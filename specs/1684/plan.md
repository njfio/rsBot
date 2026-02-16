# Issue 1684 Plan

Status: Reviewed

## Approach

1. Create module directory `crates/tau-provider/src/auth_commands_runtime/`.
2. Move shared runtime helpers to `shared_runtime_core.rs`.
3. Move provider-specific login-ready branches to:
   - `google_backend.rs`
   - `openai_backend.rs`
   - `anthropic_backend.rs`
4. Keep `auth_commands_runtime.rs` as composition surface by importing extracted functions.
5. Add split harness script and run scoped checks.

## Affected Areas

- `crates/tau-provider/src/auth_commands_runtime.rs`
- `crates/tau-provider/src/auth_commands_runtime/shared_runtime_core.rs`
- `crates/tau-provider/src/auth_commands_runtime/google_backend.rs`
- `crates/tau-provider/src/auth_commands_runtime/openai_backend.rs`
- `crates/tau-provider/src/auth_commands_runtime/anthropic_backend.rs`
- `scripts/dev/test-auth-commands-runtime-domain-split.sh`
- `specs/1684/*`

## Risks And Mitigations

- Risk: output drift in provider-specific formatting.
  - Mitigation: move provider bodies verbatim and keep runtime entrypoint call sites unchanged.
- Risk: visibility/import regressions after extraction.
  - Mitigation: use `pub(super)` boundaries and run strict clippy/tests.
- Risk: partial extraction leaving monolith unchanged.
  - Mitigation: enforce module markers + moved-helper absence in split harness script.

## ADR

No dependency/protocol architecture change; ADR not required.
