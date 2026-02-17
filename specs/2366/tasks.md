# Issue 2366 Tasks — G8 Local Embedding Provider Mode

## Ordered Tasks

1. T1 (Conformance-first tests)
   - Add failing tests for C-01..C-04 with `spec_cXX_*` names.
   - Tiers: Conformance, Unit, Regression.
   - Dependency: none.

2. T2 (Implementation: policy/config selection)
   - Update local mode detection in `ToolPolicy::memory_embedding_provider_config`.
   - Ensure remote config path remains unchanged.
   - Tiers: Unit, Functional.
   - Dependency: T1.

3. T3 (Implementation: runtime fallback verification)
   - Ensure local-mode runtime failure path returns deterministic fallback.
   - Tiers: Integration, Regression.
   - Dependency: T2.

4. T4 (Verify)
   - Run scoped fmt/clippy/tests on touched crates.
   - Capture red/green evidence and tier matrix for PR.
   - Dependency: T3.

## Tier Coverage Plan

- Unit: policy extraction + provider preservation behavior.
- Functional: embedding resolution under local/remote/default modes.
- Conformance: C-01..C-04 direct tests.
- Integration: runtime local failure fallback path.
- Regression: default behavior unchanged and remote path unchanged.
- Property/Contract/Snapshot/Fuzz/Mutation/Performance: N/A for this narrow
  slice unless touched code introduces invariant-heavy logic (justify in PR).
