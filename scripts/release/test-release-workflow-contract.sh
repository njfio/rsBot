#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
cd "${REPO_ROOT}"

RELEASE_WORKFLOW=".github/workflows/release.yml"
CI_WORKFLOW=".github/workflows/ci.yml"

assert_contains_file() {
  local file_path="$1"
  local needle="$2"
  local label="$3"
  if ! grep -Fq "${needle}" "${file_path}"; then
    echo "assertion failed (${label}): '${needle}' not found in ${file_path}" >&2
    exit 1
  fi
}

# Unit: windows arm64 release matrix entry must exist.
assert_contains_file "${RELEASE_WORKFLOW}" "target: aarch64-pc-windows-msvc" "release matrix target"
assert_contains_file "${RELEASE_WORKFLOW}" "platform: windows-arm64" "release matrix platform"
assert_contains_file "${RELEASE_WORKFLOW}" "archive_ext: zip" "windows arm64 archive format"

# Functional: cross-arch targets must use deterministic smoke skip mode and explicit reason.
assert_contains_file "${RELEASE_WORKFLOW}" "smoke_mode: skip" "cross-arch smoke skip mode"
assert_contains_file "${RELEASE_WORKFLOW}" "release_smoke_reason=" "smoke skip reason logging"
assert_contains_file "${RELEASE_WORKFLOW}" "cross_arch_windows_arm64_on_amd64_runner" "windows arm64 smoke reason"

# Regression: protect known cross-arch lanes from accidental runnable-smoke regressions.
assert_contains_file "${RELEASE_WORKFLOW}" "cross_arch_linux_arm64_on_amd64_runner" "linux arm64 smoke reason"
assert_contains_file "${RELEASE_WORKFLOW}" "cross_arch_macos_amd64_on_arm64_runner" "macos amd64 smoke reason"

# Integration: CI cross-platform smoke matrix must compile windows arm64 target.
assert_contains_file "${CI_WORKFLOW}" "name: windows-arm64" "ci cross-platform lane"
assert_contains_file "${CI_WORKFLOW}" "target: aarch64-pc-windows-msvc" "ci cross-platform target"
assert_contains_file "${CI_WORKFLOW}" "cargo build --release -p tau-coding-agent --target" "ci compile smoke command"

echo "release workflow contract tests passed"
