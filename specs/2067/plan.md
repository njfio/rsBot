# Plan #2067

Status: Implemented
Spec: specs/2067/spec.md

## Approach

1. Extract the operator-control summary/diff workflow block from
   `channel_store_admin.rs` to
   `channel_store_admin/operator_control_helpers.rs`.
2. Keep behavior stable by preserving function signatures/logic and using
   module-level imports rather than semantic rewrites.
3. Keep test visibility intact for existing `channel_store_admin::tests` by
   selectively importing helper functions into parent scope.
4. Tighten and run guardrail + targeted behavior tests.

## Affected Modules

- `crates/tau-ops/src/channel_store_admin.rs`
- `crates/tau-ops/src/channel_store_admin/operator_control_helpers.rs`
- `scripts/dev/test-channel-store-admin-domain-split.sh`

## Risks and Mitigations

- Risk: helper extraction changes operator-control summary/diff semantics.
  - Mitigation: preserve logic verbatim and run targeted operator tests.
- Risk: test module can no longer access previously local helper functions.
  - Mitigation: explicit helper imports for test-only symbols.

## Interfaces and Contracts

- Runtime entrypoint retained:
  `pub fn execute_channel_store_admin_command(cli: &Cli) -> Result<()>`
- Guardrail contract:
  `scripts/dev/test-channel-store-admin-domain-split.sh` enforces file budget
  and extraction markers.

## ADR References

- Not required.
