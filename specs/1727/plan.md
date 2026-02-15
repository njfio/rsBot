# Issue 1727 Plan

Status: Reviewed

## Approach

1. Introduce a focused RL lifecycle authorization helper module in
   `tau-access` with:
   - lifecycle action parser
   - stable action-key mapping
   - RBAC authorization wrapper methods
   - enforcement helper that fails closed on deny
2. Reuse existing `authorize_action_for_principal_with_policy_path` so no new
   RBAC schema or evaluator logic is introduced.
3. Keep error messages deterministic and operator-actionable.
4. Validate via scoped `tau-access` tests and conformance mapping.

## Affected Areas

- `crates/tau-access/src/rl_control_plane.rs`
- `crates/tau-access/src/lib.rs`
- `specs/1727/spec.md`
- `specs/1727/plan.md`
- `specs/1727/tasks.md`

## Risks And Mitigations

- Risk: drift between lifecycle action parser and action-key mapping.
  - Mitigation: explicit unit + regression tests across all action variants.
- Risk: ambiguous denial diagnostics slowing operator remediation.
  - Mitigation: include principal/action details in enforcement errors.
- Risk: accidental behavior change in default-policy path handling.
  - Mitigation: delegate to existing RBAC authorization helpers unchanged.

## ADR

No architectural dependency or protocol change; ADR not required.
