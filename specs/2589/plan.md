# Plan #2589

## Approach
1. Introduce a typed default-importance profile model shared by `tau-memory` and `tau-tools` policy wiring.
2. Resolve overrides in tool policy build path with strict bound validation and JSON diagnostics output.
3. Pass resolved profile into `FileMemoryStore` and apply profile fallback when `importance` is omitted.
4. Add conformance/regression tests across `tau-tools` + `tau-memory` for parse, write fallback, and invalid-config handling.
5. Update roadmap checklist line and capture closure evidence in #2590.

## Affected Modules
- `crates/tau-memory/src/runtime.rs`
- `crates/tau-tools/src/tools/registry_core.rs`
- `crates/tau-tools/src/tool_policy_config.rs`
- `crates/tau-tools/src/tools/memory_tools.rs`
- `crates/tau-tools/src/tools/tests.rs`
- `tasks/spacebot-comparison.md`

## Risks & Mitigations
- Risk: invalid runtime values silently degrade memory quality.
  - Mitigation: fail-closed validation for all override inputs.
- Risk: behavior drift for existing callers.
  - Mitigation: default profile remains identical to prior hardcoded defaults when no overrides are set.

## Interfaces / Contracts
- Extend tool policy JSON contract with `memory_default_importance_profile` object.
- Preserve `memory_write` interface shape; only fallback behavior changes when policy overrides are configured.
