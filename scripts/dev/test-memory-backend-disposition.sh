#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"

runtime_file="crates/tau-memory/src/runtime.rs"
docs_file="docs/guides/memory-ops.md"

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "${haystack}" != *"${needle}"* ]]; then
    echo "assertion failed (${label}): expected to find '${needle}'"
    exit 1
  fi
}

runtime_contents="$(cat "${runtime_file}")"
docs_contents="$(cat "${docs_file}")"

# Functional contract: accepted env backends remain auto/jsonl/sqlite only.
assert_contains "${runtime_contents}" 'env_backend != "auto" && env_backend != "jsonl" && env_backend != "sqlite"' \
  'functional accepted backend matrix'

# Regression contract: invalid env values route through deterministic fallback reason.
assert_contains "${runtime_contents}" 'MEMORY_STORAGE_REASON_ENV_INVALID_FALLBACK' \
  'regression invalid fallback reason constant'
assert_contains "${runtime_contents}" 'regression_memory_store_treats_postgres_env_backend_as_invalid_and_falls_back' \
  'regression postgres fallback test presence'

# Functional docs contract: runbook explicitly calls out postgres unsupported behavior.
assert_contains "${docs_contents}" '`postgres` backend is unsupported and falls back to inferred backend with reason `memory_storage_backend_env_invalid_fallback`.' \
  'functional docs postgres unsupported note'

echo "memory-backend-disposition tests passed"
