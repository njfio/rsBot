# Plan #2553

1. Add local embedding backend plumbing in `tau-memory` with FastEmbed as local provider implementation.
2. Keep explicit deterministic fallback to hash embeddings for local backend load/inference failures.
3. Update local provider config default model in `tau-tools` to `BAAI/bge-small-en-v1.5`.
4. Add conformance/regression tests first for:
   - local default model resolution,
   - local success metadata path,
   - local failure fallback path,
   - remote non-regression behavior.
5. Run verification gates and live validation evidence package.

## Risks
- FastEmbed model initialization may require external model artifacts and be unavailable in CI.
- New local backend path could regress existing remote provider behavior.

## Mitigations
- Keep local backend path fail-closed to existing hash embedding behavior.
- Introduce deterministic test seam for local backend success/failure without network/model download dependence.
- Preserve remote provider branch and validate with explicit regression tests.
