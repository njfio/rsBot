# Spec #2553 - Task: implement FastEmbed local embedding provider for tau-memory

Status: Implemented

## Problem Statement
`G8` in `tasks/spacebot-comparison.md` requires local embeddings as the default memory embedding path. Tau currently supports remote provider embeddings and hash fallback, but local mode (`provider=local`) still resolves to hash-only behavior.

## Acceptance Criteria
### AC-1 Local provider defaults to FastEmbed model contract
Given embedding provider is configured as `local` without explicit model override, when provider config resolves, then model defaults to `BAAI/bge-small-en-v1.5`.

### AC-2 Local provider can emit non-hash embeddings when backend succeeds
Given local provider backend is available, when memory write computes embeddings, then stored embedding metadata reports local provider success and non-empty normalized vectors.

### AC-3 Local provider fails closed to hash embeddings
Given local provider backend initialization or inference fails, when memory write/search computes embeddings, then behavior falls back to deterministic hash embeddings with stable failure reason code and no panic.

### AC-4 Existing remote provider behavior remains intact
Given provider is `openai`/`openai-compatible`, when memory embedding computation runs, then existing remote embedding success/fallback semantics remain unchanged.

## Scope
In scope:
- `tau-memory` local embedding backend integration and fallback behavior.
- `tau-tools` local provider default model contract.
- Conformance/regression tests for local-success, local-fallback, and remote non-regression.

Out of scope:
- Profile UX redesign.
- Graph/hybrid ranking algorithm changes outside embedding generation.

## Conformance Cases
- C-01 (AC-1): `spec_2553_c01_memory_embedding_provider_config_defaults_local_model_to_fastembed`
- C-02 (AC-2): `integration_spec_2553_c02_memory_write_local_provider_success_records_local_embedding_metadata`
- C-03 (AC-3): `regression_spec_2553_c03_memory_write_local_provider_failure_falls_back_to_hash_embedding`
- C-04 (AC-4): `regression_spec_2553_c04_remote_embedding_provider_path_preserves_existing_semantics`

## Success Metrics
- C-01..C-04 pass.
- `cargo fmt --check`, scoped `clippy`, scoped `tau-memory`/`tau-tools` tests, mutation in diff, workspace `cargo test -j 1`, and live validation script pass.
