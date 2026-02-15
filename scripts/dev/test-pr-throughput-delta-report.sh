#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DELTA_SCRIPT="${SCRIPT_DIR}/pr-throughput-delta-report.sh"

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

baseline_json="${tmp_dir}/baseline.json"
current_json="${tmp_dir}/current.json"
output_md="${tmp_dir}/delta.md"
output_json="${tmp_dir}/delta.json"
unknown_current_json="${tmp_dir}/unknown-current.json"
unknown_md="${tmp_dir}/delta-unknown.md"
unknown_out="${tmp_dir}/delta-unknown.json"

cat >"${baseline_json}" <<'EOF'
{
  "schema_version": 1,
  "generated_at": "2026-02-15T00:00:00Z",
  "window": {
    "merged_pr_count": 30
  },
  "metrics": {
    "pr_age": { "avg_seconds": 300.0 },
    "review_latency": { "avg_seconds": 120.0 },
    "merge_interval": { "avg_seconds": 600.0 }
  }
}
EOF

cat >"${current_json}" <<'EOF'
{
  "schema_version": 1,
  "generated_at": "2026-02-16T00:00:00Z",
  "window": {
    "merged_pr_count": 28
  },
  "metrics": {
    "pr_age": { "avg_seconds": 240.0 },
    "review_latency": { "avg_seconds": 150.0 },
    "merge_interval": { "avg_seconds": 600.0 }
  }
}
EOF

cat >"${unknown_current_json}" <<'EOF'
{
  "schema_version": 1,
  "generated_at": "2026-02-16T00:00:00Z",
  "window": {
    "merged_pr_count": 0
  },
  "metrics": {
    "pr_age": { "avg_seconds": null },
    "review_latency": { "avg_seconds": null },
    "merge_interval": { "avg_seconds": null }
  }
}
EOF

# Functional: computes improved/regressed/flat deltas from fixture JSON.
"${DELTA_SCRIPT}" \
  --quiet \
  --baseline-json "${baseline_json}" \
  --current-json "${current_json}" \
  --reporting-interval weekly \
  --generated-at "2026-02-16T12:00:00Z" \
  --output-md "${output_md}" \
  --output-json "${output_json}"

if [[ ! -f "${output_md}" ]]; then
  echo "assertion failed (functional markdown output): missing ${output_md}" >&2
  exit 1
fi
if [[ ! -f "${output_json}" ]]; then
  echo "assertion failed (functional json output): missing ${output_json}" >&2
  exit 1
fi

json_content="$(cat "${output_json}")"
md_content="$(cat "${output_md}")"

assert_equals "1" "$(jq -r '.schema_version' <<<"${json_content}")" "functional schema version"
assert_equals "weekly" "$(jq -r '.reporting_interval' <<<"${json_content}")" "functional interval"
assert_equals "improved" "$(jq -r '.delta.pr_age.status' <<<"${json_content}")" "functional pr-age status"
assert_equals "regressed" "$(jq -r '.delta.review_latency.status' <<<"${json_content}")" "functional review-latency status"
assert_equals "flat" "$(jq -r '.delta.merge_interval.status' <<<"${json_content}")" "functional merge-interval status"
assert_equals "1" "$(jq -r '.summary.improved_metrics' <<<"${json_content}")" "functional improved count"
assert_equals "1" "$(jq -r '.summary.regressed_metrics' <<<"${json_content}")" "functional regressed count"
assert_equals "1" "$(jq -r '.summary.flat_metrics' <<<"${json_content}")" "functional flat count"
assert_contains "${md_content}" "| PR age (created -> merged) | 5.00m | 4.00m | -1.00m | -20.00% | improved |" "functional markdown improved row"
assert_contains "${md_content}" "| Review latency (created -> first review) | 2.00m | 2.50m | 30s | +25.00% | regressed |" "functional markdown regressed row"

# Regression: handles unknown deltas when current averages are null.
"${DELTA_SCRIPT}" \
  --quiet \
  --baseline-json "${baseline_json}" \
  --current-json "${unknown_current_json}" \
  --reporting-interval weekly \
  --generated-at "2026-02-16T12:00:00Z" \
  --output-md "${unknown_md}" \
  --output-json "${unknown_out}"

assert_equals "3" "$(jq -r '.summary.unknown_metrics' <"${unknown_out}")" "regression unknown count"
assert_contains "$(cat "${unknown_md}")" "| PR age (created -> merged) | 5.00m | n/a | n/a | n/a | unknown |" "regression unknown markdown row"

echo "pr-throughput-delta-report tests passed"
