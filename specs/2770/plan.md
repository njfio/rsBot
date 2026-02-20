# Plan: Issue #2770 - G10 serenity dependency and tau-discord-runtime foundation

## Approach
1. Add RED tests/checks for workspace crate wiring and existing Discord behavior smoke expectations.
2. Introduce `serenity` dependency in the minimal required scope.
3. Create `tau-discord-runtime` crate (or approved equivalent) with baseline compile path and interfaces.
4. Wire workspace manifests and any required module exports.
5. Run scoped + crate regression tests and update checklist evidence.

## Affected Modules
- `Cargo.toml` (workspace)
- `crates/` (new `tau-discord-runtime` or approved equivalent)
- Existing Discord integration surfaces in `tau-multi-channel` as needed
- `tasks/spacebot-comparison.md`

## Risks / Mitigations
- Risk: dependency footprint or compile regressions.
  - Mitigation: minimal feature flags and scoped compile/test gates.
- Risk: runtime boundary introduces behavior drift.
  - Mitigation: preserve existing integration tests and add regression checks.

## Interfaces / Contracts
- New crate/module public interface must remain intentionally minimal (bootstrap surface only).
- Existing runtime behavior contracts remain unchanged in this slice.

## ADR
Likely required for new dependency + boundary decision; author if dependency strategy or crate split introduces architectural commitment.
