# Issue 2366 Spec — G8 Local Embedding Provider Mode

Status: Accepted

## Problem Statement

`tasks/spacebot-comparison.md` identifies gap `G8` (Local Embeddings). Tau has
provider-backed embeddings and deterministic hash fallback, but no explicit
first-class `local` embedding provider mode in runtime policy selection.
Current policy extraction can require remote provider fields and prevents clear
operator intent for "local only" memory embedding behavior.

## Scope

In scope:

- Add explicit local embedding provider selection in runtime/tool policy flow.
- Ensure local mode works without remote API credentials.
- Preserve remote provider embedding behavior when provider config is supplied.
- Preserve deterministic fallback behavior on local embedding failure.

Out of scope:

- Adding new embedding ML dependencies (e.g., `fastembed`) in this issue.
- Bulk ingestion or memory graph features.
- Provider billing/caching changes.

## Acceptance Criteria

### AC-1: Explicit Local Mode Selection

Given runtime/tool policy selects local embedding mode,
When memory embedding configuration is resolved,
Then the system returns a local embedding provider config without requiring
remote provider API fields.

### AC-2: Local Mode Hash Backend

Given local embedding mode is selected,
When memory embeddings are generated for write/search operations,
Then deterministic hash embeddings are used with no remote API dependency.

### AC-3: Remote Mode Preservation

Given remote provider embedding configuration is fully specified,
When memory embedding configuration is resolved,
Then provider-backed embedding flow remains active and behavior is unchanged.

### AC-4: Backward-Compatible Defaults

Given no embedding provider policy is set,
When memory embedding operations run,
Then existing deterministic fallback/default behavior remains unchanged.

## Conformance Cases

| Case | Maps To | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Conformance/Unit | Tool policy with `provider_api=local` and no API key/base | Resolve memory embedding config | Returns `MemoryEmbeddingProviderConfig` with `provider=local` and no remote-field requirement |
| C-02 | AC-2 | Conformance/Integration | Local provider config selected | Execute memory write path | Returns hash embedding backend metadata (`hash-fnv1a` / hash-only reason) |
| C-03 | AC-3 | Conformance/Unit | Tool policy with provider config (e.g., OpenAI-compatible fields) | Resolve memory embedding config | Returns provider config for remote embeddings exactly as before |
| C-04 | AC-4 | Regression/Functional | No embedding provider policy set | Embed text path executes | Existing fallback/default path is unchanged |

## Success Metrics / Observable Signals

- New conformance tests `spec_c01`..`spec_c04` pass.
- Existing embedding tests in `tau-memory` and `tau-agent-core` remain green.
- No behavior regression in provider-based embedding integration tests.

## AC → Conformance → Test Mapping

- AC-1 → C-01 → `spec_c01_local_mode_config_without_remote_fields`
- AC-2 → C-02 → `spec_c02_local_mode_runtime_failure_falls_back`
- AC-3 → C-03 → `spec_c03_remote_provider_config_preserved`
- AC-4 → C-04 → `spec_c04_default_behavior_unchanged_without_policy`
