# Plan: Issue #2657 - Encrypted API-key persistence via SecretStore (G20 phase 2)

## Approach
1. Identify API-key persistence surfaces that currently write plaintext profile/config values.
2. Add RED tests proving plaintext persistence is rejected and encrypted SecretStore retrieval works.
3. Integrate API-key persistence/retrieval with SecretStore-backed credential-store paths.
4. Keep explicit `none`/`keyed` modes deterministic and covered by regression tests.
5. Run scoped verification and update roadmap evidence.

## Affected Modules
- `crates/tau-provider/src/` (auth and credential-store paths)
- `crates/tau-coding-agent/src/tests/auth_provider/`
- `tasks/spacebot-comparison.md`
- `specs/milestones/m108/index.md`
- `specs/2657/spec.md`
- `specs/2657/plan.md`
- `specs/2657/tasks.md`

## Risks / Mitigations
- Risk: migration could alter existing auth fallback semantics.
  - Mitigation: conformance/regression tests around existing none/keyed/auth-resolution behavior.
- Risk: path discovery misses a plaintext write location.
  - Mitigation: audit targeted modules with test assertions on persisted artifacts.

## Interfaces / Contracts
- Extend existing provider/auth persistence contracts to route through encrypted SecretStore APIs.
- Preserve public CLI/profile behavior where feasible.

## ADR
- Not required unless dependency/storage engine decision changes during implementation.
