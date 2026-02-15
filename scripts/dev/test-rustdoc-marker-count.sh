#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT_UNDER_TEST="${SCRIPT_DIR}/rustdoc-marker-count.sh"

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for test-rustdoc-marker-count.sh" >&2
  exit 1
fi

assert_equals() {
  local expected="$1"
  local actual="$2"
  local label="$3"
  if [[ "${expected}" != "${actual}" ]]; then
    echo "assertion failed (${label}): expected '${expected}' got '${actual}'" >&2
    exit 1
  fi
}

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

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

fixture_root="${tmp_dir}/fixture"
mkdir -p "${fixture_root}/crates/alpha/src" "${fixture_root}/crates/beta/src"

cat >"${fixture_root}/crates/alpha/src/lib.rs" <<'EOF'
//! alpha crate docs
/// alpha public api
pub fn alpha() {}
EOF

cat >"${fixture_root}/crates/alpha/src/internal.rs" <<'EOF'
/// alpha internal behavior docs
fn internal() {}
EOF

cat >"${fixture_root}/crates/beta/src/lib.rs" <<'EOF'
//! beta docs one
//! beta docs two
/// beta public api
pub struct Beta;
EOF

output_json="${tmp_dir}/marker-count.json"
output_md="${tmp_dir}/marker-count.md"

bash -n "${SCRIPT_UNDER_TEST}"

stdout_capture="$(
  "${SCRIPT_UNDER_TEST}" \
    --repo-root "${fixture_root}" \
    --scan-root "crates" \
    --output-json "${output_json}" \
    --output-md "${output_md}" \
    --generated-at "2026-02-15T18:00:00Z"
)"

if [[ ! -f "${output_json}" ]]; then
  echo "assertion failed (functional): expected JSON artifact output" >&2
  exit 1
fi
if [[ ! -f "${output_md}" ]]; then
  echo "assertion failed (functional): expected Markdown artifact output" >&2
  exit 1
fi

assert_contains "${stdout_capture}" "total_markers=6" "functional total marker summary"
assert_contains "${stdout_capture}" "alpha=3" "functional per-crate alpha summary"
assert_contains "${stdout_capture}" "beta=3" "functional per-crate beta summary"

assert_equals "1" "$(jq -r '.schema_version' "${output_json}")" "conformance schema version"
assert_equals "2026-02-15T18:00:00Z" "$(jq -r '.generated_at' "${output_json}")" "conformance generated-at"
assert_equals "6" "$(jq -r '.total_markers' "${output_json}")" "functional total markers"
assert_equals "2" "$(jq -r '.crates | length' "${output_json}")" "functional crate count"
assert_equals "alpha" "$(jq -r '.crates[0].crate' "${output_json}")" "conformance sorted crate order alpha"
assert_equals "beta" "$(jq -r '.crates[1].crate' "${output_json}")" "conformance sorted crate order beta"
assert_equals "3" "$(jq -r '.crates[0].markers' "${output_json}")" "functional alpha markers"
assert_equals "3" "$(jq -r '.crates[1].markers' "${output_json}")" "functional beta markers"

assert_contains "$(cat "${output_md}")" "# M23 Rustdoc Marker Count" "functional markdown title"
assert_contains "$(cat "${output_md}")" "| alpha | 3 | 2 |" "functional markdown crate row alpha"
assert_contains "$(cat "${output_md}")" "| beta | 3 | 1 |" "functional markdown crate row beta"

set +e
unknown_option_output="$("${SCRIPT_UNDER_TEST}" --unknown-option 2>&1)"
unknown_option_code=$?
set -e
assert_equals "1" "${unknown_option_code}" "regression unknown option exit"
assert_contains "${unknown_option_output}" "unknown option '--unknown-option'" "regression unknown option message"

set +e
missing_scan_output="$(
  "${SCRIPT_UNDER_TEST}" \
    --repo-root "${fixture_root}" \
    --scan-root does-not-exist \
    --quiet 2>&1
)"
missing_scan_code=$?
set -e
assert_equals "1" "${missing_scan_code}" "regression missing scan root exit"
assert_contains "${missing_scan_output}" "scan root not found" "regression missing scan root message"

echo "rustdoc-marker-count tests passed"
