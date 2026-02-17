# Issue 2366 Plan — G8 Local Embedding Provider Mode

## Approach

Implement a narrow policy/configuration slice:

1. Extend embedding policy resolution to recognize explicit `local` provider
   intent.
2. Keep provider-backed path unchanged for fully configured remote providers.
3. Reuse existing deterministic fallback embedding behavior for resilience.
4. Add conformance-first tests for all AC mappings.

## Affected Modules

- `crates/tau-tools/src/tools/registry_core.rs`
  - `ToolPolicy::memory_embedding_provider_config` local mode selection behavior.
- `crates/tau-memory/src/runtime.rs`
  - Runtime embedding resolution/fallback tests and any small glue needed.
- `crates/tau-memory/src/lib.rs` (if type exposure adjustments are required).
- `crates/tau-agent-core` tests (if policy integration coverage is housed here).

## Risks and Mitigations

- Risk: Regressing provider-backed embedding configuration.
  - Mitigation: Explicit regression test for remote config preservation (C-03).
- Risk: Local-mode failure surfacing as hard errors.
  - Mitigation: Conformance test for deterministic fallback (C-02).
- Risk: Ambiguous defaults across existing callers.
  - Mitigation: Backward-compatibility regression case (C-04).

## Interfaces / Contracts

- Continue using existing `MemoryEmbeddingProviderConfig` contract.
- Local mode identified by `provider == "local"` through policy extraction.
- No schema or wire-format changes in this issue.

## ADR

No ADR required for this slice (no new dependency and no architecture-level
interface redesign).
