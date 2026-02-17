#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
cd "${REPO_ROOT}"

RENDER_SCRIPT="scripts/release/render-homebrew-formula.sh"

assert_contains_file() {
  local file_path="$1"
  local needle="$2"
  local label="$3"
  if ! grep -Fq "${needle}" "${file_path}"; then
    echo "assertion failed (${label}): '${needle}' not found in ${file_path}" >&2
    exit 1
  fi
}

assert_url_sha_pair() {
  local file_path="$1"
  local expected_url="$2"
  local expected_sha="$3"
  local label="$4"

  if ! awk -v url="${expected_url}" -v sha="${expected_sha}" '
      index($0, "url \"" url "\"") { saw_url = 1; next }
      saw_url && index($0, "sha256 \"" sha "\"") { found = 1; exit 0 }
      saw_url && index($0, "url \"") { saw_url = 0 }
      END { exit(found ? 0 : 1) }
    ' "${file_path}"; then
    echo "assertion failed (${label}): url/sha pair not found for ${expected_url}" >&2
    exit 1
  fi
}

temp_dir="$(mktemp -d)"
trap 'rm -rf "${temp_dir}"' EXIT

checksum_manifest="${temp_dir}/SHA256SUMS"
formula_output="${temp_dir}/tau.rb"

cat > "${checksum_manifest}" <<'EOF'
1111111111111111111111111111111111111111111111111111111111111111  dist/tau-coding-agent-linux-amd64.tar.gz
2222222222222222222222222222222222222222222222222222222222222222  dist/tau-coding-agent-linux-arm64.tar.gz
3333333333333333333333333333333333333333333333333333333333333333  dist/tau-coding-agent-macos-amd64.tar.gz
4444444444444444444444444444444444444444444444444444444444444444  dist/tau-coding-agent-macos-arm64.tar.gz
5555555555555555555555555555555555555555555555555555555555555555  dist/tau-coding-agent-windows-amd64.zip
6666666666666666666666666666666666666666666666666666666666666666  dist/tau-coding-agent-windows-arm64.zip
EOF

"${RENDER_SCRIPT}" "v9.9.9" "${checksum_manifest}" "acme/tau" > "${formula_output}"

# Conformance: formula scaffold and smoke test body.
assert_contains_file "${formula_output}" "class Tau < Formula" "formula class"
assert_contains_file "${formula_output}" "on_macos do" "formula macOS stanza"
assert_contains_file "${formula_output}" "on_linux do" "formula Linux stanza"
assert_contains_file "${formula_output}" "bin.install \"tau-coding-agent\" => \"tau\"" "binary install alias"
assert_contains_file "${formula_output}" "test do" "formula test block"
assert_contains_file "${formula_output}" "system \"#{bin}/tau\", \"--help\"" "formula smoke command"

# Conformance: per-platform URL + checksum mapping.
assert_url_sha_pair "${formula_output}" \
  "https://github.com/acme/tau/releases/download/v9.9.9/tau-coding-agent-macos-arm64.tar.gz" \
  "4444444444444444444444444444444444444444444444444444444444444444" \
  "macos arm64 mapping"
assert_url_sha_pair "${formula_output}" \
  "https://github.com/acme/tau/releases/download/v9.9.9/tau-coding-agent-macos-amd64.tar.gz" \
  "3333333333333333333333333333333333333333333333333333333333333333" \
  "macos amd64 mapping"
assert_url_sha_pair "${formula_output}" \
  "https://github.com/acme/tau/releases/download/v9.9.9/tau-coding-agent-linux-arm64.tar.gz" \
  "2222222222222222222222222222222222222222222222222222222222222222" \
  "linux arm64 mapping"
assert_url_sha_pair "${formula_output}" \
  "https://github.com/acme/tau/releases/download/v9.9.9/tau-coding-agent-linux-amd64.tar.gz" \
  "1111111111111111111111111111111111111111111111111111111111111111" \
  "linux amd64 mapping"

echo "homebrew formula contract tests passed"
