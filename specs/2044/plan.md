# Plan #2044

Status: Implemented
Spec: specs/2044/spec.md

## Approach

1. Complete split-map planning artifacts and deterministic contracts in
   `#2066`.
2. Execute low-risk extraction of operator-control summary/diff domain in
   `#2067` to clear file-size budget quickly.
3. Keep CLI/report behavior stable by preserving helper logic and verifying
   targeted runtime tests.
4. Tighten domain split guardrail to enforce `<2200` threshold.

## Affected Modules

- `crates/tau-ops/src/channel_store_admin.rs`
- `crates/tau-ops/src/channel_store_admin/operator_control_helpers.rs`
- `scripts/dev/test-channel-store-admin-domain-split.sh`
- split-map artifacts under `scripts/dev/`, `tasks/schemas/`, `tasks/reports/`,
  and `docs/guides/`

## Risks and Mitigations

- Risk: extraction causes subtle operator-control summary drift.
  - Mitigation: preserve logic and run focused summary/diff test slices.
- Risk: governance artifacts drift from implementation.
  - Mitigation: enforce split-map contract tests and keep spec artifacts updated
    to Implemented.

## Interfaces and Contracts

- Stable command contract:
  `execute_channel_store_admin_command(cli: &Cli) -> Result<()>`
- Guardrails:
  `scripts/dev/test-channel-store-admin-domain-split.sh`,
  `scripts/dev/test-channel-store-admin-split-map.sh`

## ADR References

- Not required.
