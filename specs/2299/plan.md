# Plan #2299

Status: Reviewed
Spec: specs/2299/spec.md

## Approach

1. Extend payload parsing in `tau-provider::model_catalog` to detect and map OpenRouter `/api/v1/models` response shape.
2. Add deterministic catalog merge helper: `merge_model_catalog_files(base, overlay)` where overlay (remote) wins by normalized key.
3. Update `load_model_catalog_with_cache` remote-success path to merge built-in + remote before cache write and return.
4. Preserve existing cache fallback semantics and diagnostics source fields.
5. Add spec-conformance tests first (RED), then implement minimal code (GREEN), then run scoped regression checks.

## Affected Modules

- `crates/tau-provider/src/model_catalog.rs`
- (tests in same module)

## Risks and Mitigations

- Risk: OpenRouter payload drift could break parser assumptions.
  - Mitigation: tolerant optional-field mapping; fail only on structurally invalid payload.
- Risk: Merge semantics can silently alter catalog precedence.
  - Mitigation: explicit tests for override behavior and retained key sets.

## Interfaces / Contracts

- `parse_model_catalog_payload` remains the entrypoint and now accepts OpenRouter payload shape.
- `load_model_catalog_with_cache` behavior remains backward-compatible while ensuring remote refresh does not erase built-in entries.
