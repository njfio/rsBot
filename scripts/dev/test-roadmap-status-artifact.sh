#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ARTIFACT_SCRIPT="${SCRIPT_DIR}/roadmap-status-artifact.sh"

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

assert_equals() {
  local expected="$1"
  local actual="$2"
  local label="$3"
  if [[ "${expected}" != "${actual}" ]]; then
    echo "assertion failed (${label}): expected '${expected}', got '${actual}'" >&2
    exit 1
  fi
}

if [[ ! -x "${ARTIFACT_SCRIPT}" ]]; then
  echo "assertion failed (unit executable): missing executable ${ARTIFACT_SCRIPT}" >&2
  exit 1
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

config_path="${tmp_dir}/config.json"
fixture_closed_path="${tmp_dir}/fixture-closed.json"
fixture_open_path="${tmp_dir}/fixture-open.json"
fixture_malformed_path="${tmp_dir}/fixture-malformed.json"
malformed_config_path="${tmp_dir}/config-malformed.json"
output_json="${tmp_dir}/roadmap-status.json"
output_md="${tmp_dir}/roadmap-status.md"

cat >"${config_path}" <<'EOF'
{
  "todo_groups": [
    { "label": "Phase Alpha", "ids": [100, 101] },
    { "label": "Phase Beta", "ids": [200] }
  ],
  "epic_ids": [200],
  "gap": {
    "core_delivery_pr_span": { "from": 10, "to": 12 },
    "child_story_task_ids": [300, 301],
    "epic_summary": "#200 (Sample Epic)"
  }
}
EOF

cat >"${fixture_closed_path}" <<'EOF'
{
  "default_state": "CLOSED",
  "issues": []
}
EOF

cat >"${fixture_open_path}" <<'EOF'
{
  "default_state": "CLOSED",
  "issues": [
    { "number": 200, "state": "OPEN" },
    { "number": 301, "state": "OPEN" }
  ]
}
EOF

cat >"${fixture_malformed_path}" <<'EOF'
{
  "default_state": "OPEN",
  "issues": { "not": "an-array" }
}
EOF

cat >"${malformed_config_path}" <<'EOF'
{
  "todo_groups": [],
  "epic_ids": [],
  "gap": {}
}
EOF

# Functional: fixture + fixed timestamp produces stable artifact payload.
"${ARTIFACT_SCRIPT}" \
  --quiet \
  --config-path "${config_path}" \
  --fixture-json "${fixture_closed_path}" \
  --generated-at "2026-02-16T00:00:00Z" \
  --output-json "${output_json}" \
  --output-md "${output_md}"

json_content="$(cat "${output_json}")"
md_content="$(cat "${output_md}")"
assert_equals "1" "$(jq -r '.schema_version' <<<"${json_content}")" "functional schema version"
assert_equals "2026-02-16T00:00:00Z" "$(jq -r '.generated_at' <<<"${json_content}")" "functional generated_at"
assert_equals "5" "$(jq -r '.summary.tracked_issue_count' <<<"${json_content}")" "functional tracked count"
assert_equals "5" "$(jq -r '.summary.closed_count' <<<"${json_content}")" "functional closed count"
assert_equals "true" "$(jq -r '.summary.all_closed' <<<"${json_content}")" "functional all_closed"
assert_contains "${md_content}" "# Roadmap Status Artifact" "functional markdown title"
assert_contains "${md_content}" "Phase Alpha" "functional markdown group row"

# Functional: deterministic rerun yields identical file hashes.
hash_before="$(shasum "${output_json}" "${output_md}")"
"${ARTIFACT_SCRIPT}" \
  --quiet \
  --config-path "${config_path}" \
  --fixture-json "${fixture_closed_path}" \
  --generated-at "2026-02-16T00:00:00Z" \
  --output-json "${output_json}" \
  --output-md "${output_md}"
hash_after="$(shasum "${output_json}" "${output_md}")"
assert_equals "${hash_before}" "${hash_after}" "functional deterministic hash"

# Regression: open states are reflected in summary and group/epic rows.
"${ARTIFACT_SCRIPT}" \
  --quiet \
  --config-path "${config_path}" \
  --fixture-json "${fixture_open_path}" \
  --generated-at "2026-02-16T00:00:00Z" \
  --output-json "${output_json}" \
  --output-md "${output_md}"
assert_equals "false" "$(jq -r '.summary.all_closed' <"${output_json}")" "regression all_closed false"
assert_equals "OPEN" "$(jq -r '.issue_states[] | select(.number == 200) | .state' <"${output_json}")" "regression open epic state"

# Regression: invalid generated-at fails closed.
if "${ARTIFACT_SCRIPT}" --quiet --config-path "${config_path}" --fixture-json "${fixture_closed_path}" --generated-at "not-a-date" --output-json "${output_json}" --output-md "${output_md}" >/dev/null 2>&1; then
  echo "assertion failed (regression invalid generated-at): expected failure" >&2
  exit 1
fi

# Regression: malformed fixture fails closed.
if "${ARTIFACT_SCRIPT}" --quiet --config-path "${config_path}" --fixture-json "${fixture_malformed_path}" --generated-at "2026-02-16T00:00:00Z" --output-json "${output_json}" --output-md "${output_md}" >/dev/null 2>&1; then
  echo "assertion failed (regression malformed fixture): expected failure" >&2
  exit 1
fi

# Regression: malformed config fails closed.
if "${ARTIFACT_SCRIPT}" --quiet --config-path "${malformed_config_path}" --fixture-json "${fixture_closed_path}" --generated-at "2026-02-16T00:00:00Z" --output-json "${output_json}" --output-md "${output_md}" >/dev/null 2>&1; then
  echo "assertion failed (regression malformed config): expected failure" >&2
  exit 1
fi

echo "roadmap-status-artifact tests passed"
