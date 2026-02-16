#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SPLIT_MAP_SCRIPT="${SCRIPT_DIR}/benchmark-artifact-split-map.sh"

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

if [[ ! -x "${SPLIT_MAP_SCRIPT}" ]]; then
  echo "assertion failed (unit executable): missing executable ${SPLIT_MAP_SCRIPT}" >&2
  exit 1
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

source_file="${tmp_dir}/benchmark_artifact.rs"
missing_source_file="${tmp_dir}/missing.rs"
output_json="${tmp_dir}/split-map.json"
output_md="${tmp_dir}/split-map.md"

cat >"${source_file}" <<'EOF'
pub fn render_report() {}
pub fn write_artifact() {}
pub fn validate_benchmark() {}
EOF

# Functional: generator emits JSON + markdown contract fields.
"${SPLIT_MAP_SCRIPT}" \
  --quiet \
  --source-file "${source_file}" \
  --target-lines 2 \
  --generated-at "2026-02-16T00:00:00Z" \
  --output-json "${output_json}" \
  --output-md "${output_md}"

if [[ ! -f "${output_json}" ]]; then
  echo "assertion failed (functional output json): missing ${output_json}" >&2
  exit 1
fi
if [[ ! -f "${output_md}" ]]; then
  echo "assertion failed (functional output md): missing ${output_md}" >&2
  exit 1
fi

assert_equals "1" "$(jq -r '.schema_version' "${output_json}")" "functional schema_version"
assert_equals "2026-02-16T00:00:00Z" "$(jq -r '.generated_at' "${output_json}")" "functional generated_at"
assert_equals "3" "$(jq -r '.current_line_count' "${output_json}")" "functional current line count"
assert_equals "1" "$(jq -r '.line_gap_to_target' "${output_json}")" "functional line gap"
assert_equals "true" "$(jq -r '(.extraction_phases | length) > 0' "${output_json}")" "functional extraction phases non-empty"
assert_equals "true" "$(jq -r '(.public_api_impact | length) > 0' "${output_json}")" "functional api impact non-empty"
assert_equals "true" "$(jq -r '(.test_migration_plan | length) > 0' "${output_json}")" "functional test migration non-empty"
assert_contains "$(cat "${output_md}")" "Benchmark Artifact Split Map" "functional markdown title"

# Regression: missing source file fails closed.
if "${SPLIT_MAP_SCRIPT}" --quiet --source-file "${missing_source_file}" --output-json "${output_json}" --output-md "${output_md}" >/dev/null 2>&1; then
  echo "assertion failed (regression missing source): expected failure" >&2
  exit 1
fi

# Regression: invalid target lines fails closed.
if "${SPLIT_MAP_SCRIPT}" --quiet --source-file "${source_file}" --target-lines 0 --output-json "${output_json}" --output-md "${output_md}" >/dev/null 2>&1; then
  echo "assertion failed (regression invalid target): expected failure" >&2
  exit 1
fi

# Functional regression: deterministic output for identical input and timestamp.
hash_before="$(shasum "${output_json}" "${output_md}")"
"${SPLIT_MAP_SCRIPT}" \
  --quiet \
  --source-file "${source_file}" \
  --target-lines 2 \
  --generated-at "2026-02-16T00:00:00Z" \
  --output-json "${output_json}" \
  --output-md "${output_md}"
hash_after="$(shasum "${output_json}" "${output_md}")"
assert_equals "${hash_before}" "${hash_after}" "functional deterministic hash"

echo "benchmark-artifact-split-map tests passed"
