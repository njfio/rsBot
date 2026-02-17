# Plan #2262

Status: Reviewed
Spec: specs/2262/spec.md

## Approach

1. Add preview2 ABI constants/pattern defaults in deployment wasm profile
   builders and compliance checks.
2. Extend compliance matcher to support wildcard/prefix module constraints for
   preview2 module namespaces.
3. Update deployment tests to assert preview2 ABI/reporting behavior.
4. Update deployment runbook and run scoped validation:
   - `cargo test -p tau-deployment`
   - `./scripts/dev/wasm-smoke.sh`

## Affected Modules

- `crates/tau-deployment/src/deployment_wasm.rs`
- `docs/guides/deployment-ops.md`
- `specs/2262/spec.md`
- `specs/2262/plan.md`
- `specs/2262/tasks.md`

## Risks and Mitigations

- Risk: migration breaks legacy preview1 artifacts.
  - Mitigation: keep explicit forbidden/default posture and clear reason codes.
- Risk: wildcard matching broadens allowed imports too far.
  - Mitigation: constrain to explicit wildcard patterns (`wasi:*`) and keep
    forbidden module checks.

## Interfaces / Contracts

- Deployment manifest/runtime constraint contract changes from preview1 default
  ABI string to preview2 pattern ABI (`wasi:*`).
- Compliance matcher contract supports wildcard/prefix entries ending in `*`.
