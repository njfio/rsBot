#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
cd "${REPO_ROOT}"

FAST_VALIDATE="${SCRIPT_DIR}/fast-validate.sh"

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "${haystack}" != *"${needle}"* ]]; then
    echo "assertion failed (${label}): expected output to contain '${needle}'" >&2
    echo "actual output:" >&2
    echo "${haystack}" >&2
    exit 1
  fi
}

assert_not_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "${haystack}" == *"${needle}"* ]]; then
    echo "assertion failed (${label}): expected output to NOT contain '${needle}'" >&2
    echo "actual output:" >&2
    echo "${haystack}" >&2
    exit 1
  fi
}

output="$(printf 'crates/tau-cli/src/cli_args.rs\n' | "${FAST_VALIDATE}" --print-packages-from-stdin)"
assert_contains "${output}" "full_workspace=0" "crate file should not force workspace"
assert_contains "${output}" "package=tau-cli" "crate file should map to tau-cli package"
assert_contains "${output}" "package=tau-coding-agent" "tau-cli impact scope should include reverse dependents"

output="$(printf 'Cargo.toml\n' | "${FAST_VALIDATE}" --print-packages-from-stdin)"
assert_contains "${output}" "full_workspace=1" "workspace manifest should force full scope"

output="$(printf 'docs/README.md\n' | "${FAST_VALIDATE}" --print-packages-from-stdin)"
assert_contains "${output}" "full_workspace=0" "docs-only change should stay package-scoped"
assert_not_contains "${output}" "package=" "docs-only change should not emit package scope"

output="$(printf 'crates/tau-cli/src/lib.rs\ncrates/tau-tools/src/lib.rs\n' | "${FAST_VALIDATE}" --print-packages-from-stdin)"
assert_contains "${output}" "package=tau-cli" "multi-crate input should include tau-cli"
assert_contains "${output}" "package=tau-tools" "multi-crate input should include tau-tools"
assert_contains "${output}" "package=tau-coding-agent" "tau-tools impact scope should include coding-agent"

echo "fast-validate scope tests passed"
