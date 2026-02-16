#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BASELINE_SCRIPT="${SCRIPT_DIR}/build-test-latency-baseline.sh"

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
    echo "assertion failed (${label}): expected '${expected}' got '${actual}'" >&2
    exit 1
  fi
}

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

fixture_path="${tmp_dir}/fixture.json"
invalid_fixture_path="${tmp_dir}/fixture-invalid.json"
markdown_path="${tmp_dir}/baseline.md"
json_path="${tmp_dir}/baseline.json"

cat >"${fixture_path}" <<'EOF'
{
  "repository": "fixture/repo",
  "source_mode": "fixture",
  "environment": {
    "os": "linux",
    "arch": "x86_64",
    "shell": "bash",
    "rustc_version": "rustc 1.86.0",
    "cargo_version": "cargo 1.86.0"
  },
  "runs": [
    { "id": "fmt", "command": "cargo fmt --check", "iteration": 1, "duration_ms": 1020, "exit_code": 0 },
    { "id": "fmt", "command": "cargo fmt --check", "iteration": 2, "duration_ms": 980, "exit_code": 0 },
    { "id": "fmt", "command": "cargo fmt --check", "iteration": 3, "duration_ms": 1000, "exit_code": 0 },

    { "id": "clippy", "command": "cargo clippy -- -D warnings", "iteration": 1, "duration_ms": 2400, "exit_code": 0 },
    { "id": "clippy", "command": "cargo clippy -- -D warnings", "iteration": 2, "duration_ms": 2350, "exit_code": 0 },
    { "id": "clippy", "command": "cargo clippy -- -D warnings", "iteration": 3, "duration_ms": 2450, "exit_code": 0 },

    { "id": "test-no-run", "command": "cargo test -p tau-github-issues-runtime --no-run", "iteration": 1, "duration_ms": 5100, "exit_code": 0 },
    { "id": "test-no-run", "command": "cargo test -p tau-github-issues-runtime --no-run", "iteration": 2, "duration_ms": 5000, "exit_code": 1 },
    { "id": "test-no-run", "command": "cargo test -p tau-github-issues-runtime --no-run", "iteration": 3, "duration_ms": 5200, "exit_code": 0 }
  ]
}
EOF

cat >"${invalid_fixture_path}" <<'EOF'
{
  "repository": "fixture/repo",
  "source_mode": "fixture",
  "environment": { "os": "linux", "arch": "x86_64", "shell": "bash" },
  "runs": [
    { "id": "fmt", "command": "cargo fmt --check", "iteration": 1, "exit_code": 0 }
  ]
}
EOF

"${BASELINE_SCRIPT}" \
  --quiet \
  --fixture-json "${fixture_path}" \
  --generated-at "2026-02-16T12:00:00Z" \
  --output-md "${markdown_path}" \
  --output-json "${json_path}"

if [[ ! -f "${markdown_path}" ]]; then
  echo "assertion failed (functional markdown output): missing ${markdown_path}" >&2
  exit 1
fi
if [[ ! -f "${json_path}" ]]; then
  echo "assertion failed (functional json output): missing ${json_path}" >&2
  exit 1
fi

json_content="$(cat "${json_path}")"
markdown_content="$(cat "${markdown_path}")"

assert_equals "1" "$(jq -r '.schema_version' <<<"${json_content}")" "functional schema version"
assert_equals "fixture" "$(jq -r '.source_mode' <<<"${json_content}")" "functional source mode"
assert_equals "3" "$(jq -r '.summary.command_count' <<<"${json_content}")" "functional command count"
assert_equals "9" "$(jq -r '.summary.run_count' <<<"${json_content}")" "functional run count"
assert_equals "1" "$(jq -r '.summary.failing_runs' <<<"${json_content}")" "functional failing run count"
assert_equals "3" "$(jq -r '.commands | length' <<<"${json_content}")" "functional command rows"
assert_equals "5100" "$(jq -r '.commands[] | select(.id == "test-no-run") | .stats.avg_ms' <<<"${json_content}")" "functional test avg"
assert_equals "2400" "$(jq -r '.commands[] | select(.id == "clippy") | .stats.avg_ms' <<<"${json_content}")" "functional clippy avg"
assert_equals "1000" "$(jq -r '.commands[] | select(.id == "fmt") | .stats.avg_ms' <<<"${json_content}")" "functional fmt avg"
assert_equals "test-no-run" "$(jq -r '.hotspots[0].id' <<<"${json_content}")" "functional hotspot rank 1"
assert_equals "clippy" "$(jq -r '.hotspots[1].id' <<<"${json_content}")" "functional hotspot rank 2"
assert_equals "fmt" "$(jq -r '.hotspots[2].id' <<<"${json_content}")" "functional hotspot rank 3"
assert_contains "${markdown_content}" "| test-no-run | 3 | 5100 | 5100 | 5000 | 5200 | 1 |" "functional markdown command row"

set +e
invalid_output="$("${BASELINE_SCRIPT}" --quiet --fixture-json "${invalid_fixture_path}" --output-md "${tmp_dir}/invalid.md" --output-json "${tmp_dir}/invalid.json" 2>&1)"
invalid_exit=$?
set -e

if [[ ${invalid_exit} -eq 0 ]]; then
  echo "assertion failed (regression invalid fixture): expected non-zero exit" >&2
  exit 1
fi
assert_contains "${invalid_output}" "duration_ms" "regression invalid fixture error"

echo "build-test-latency-baseline tests passed"
