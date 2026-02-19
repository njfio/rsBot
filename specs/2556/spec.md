# Spec #2556 - Task: default profile memory embedding provider to local

Status: Implemented

## Problem Statement
Tau currently requires explicit memory embedding provider selection before local provider mode is active. `G8` requires local embeddings to be the default profile/policy behavior while keeping explicit remote-provider configuration functional.

## Acceptance Criteria
### AC-1 Default tool policy resolves memory embedding provider to local
Given runtime policy is built with default settings and no memory embedding provider override, when policy is resolved, then `memory_embedding_provider` defaults to `local` and provider config resolves without requiring remote API fields.

### AC-2 Explicit remote provider override still works
Given runtime policy default is local, when `TAU_MEMORY_EMBEDDING_PROVIDER` and required remote embedding fields are explicitly configured, then provider config resolves to the explicit remote provider and preserves remote model/base/key values.

### AC-3 Startup safety policy surfaces default-local behavior
Given startup safety policy resolves with no embedding-provider override, when startup policy diagnostics are produced, then tool-policy diagnostics report `memory_embedding_provider=local`.

## Scope
In scope:
- `ToolPolicy` default memory embedding provider wiring.
- Tool policy env/override resolution for local-default + explicit-remote behavior.
- Startup policy and conformance/regression tests for default-local behavior.
- Documentation updates for embedding provider defaults.

Out of scope:
- FastEmbed model loading implementation details.
- New embedding backend protocols.

## Conformance Cases
- C-01 (AC-1): `spec_c04_memory_embedding_provider_config_defaults_to_local`
- C-02 (AC-1): `integration_build_tool_policy_defaults_memory_embedding_provider_local`
- C-03 (AC-2): `integration_build_tool_policy_reads_memory_embedding_env_without_exposing_keys`
- C-04 (AC-3): `regression_resolve_startup_safety_policy_defaults_memory_embedding_provider_local`

## Success Metrics
- C-01..C-04 pass.
- `cargo fmt --check`, `cargo clippy -- -D warnings`, scoped tests for touched crates, mutation in diff, live validation, and workspace gate pass.
