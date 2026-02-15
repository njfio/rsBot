#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
SAFETY_SCRIPT="${SCRIPT_DIR}/safety-smoke.sh"
INDEX_SCRIPT="${SCRIPT_DIR}/index.sh"

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

mock_binary="${tmp_dir}/tau-coding-agent"
cat >"${mock_binary}" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

if [[ " $* " == *" --prompt "* ]]; then
  echo 'Error: safety policy blocked inbound_message: reason_codes=["prompt_injection.ignore_instructions"]' >&2
  exit 1
fi

echo "mock-ok"
exit 0
EOF
chmod +x "${mock_binary}"

bash -n "${SAFETY_SCRIPT}"
bash -n "${INDEX_SCRIPT}"

wrapper_output="$(
  "${SAFETY_SCRIPT}" \
    --repo-root "${REPO_ROOT}" \
    --binary "${mock_binary}" \
    --skip-build 2>&1
)"
assert_contains "${wrapper_output}" "[demo:safety-smoke] PASS safety-prompt-injection-block" "functional wrapper pass marker"
assert_contains "${wrapper_output}" "[demo:safety-smoke] summary: total=1 passed=1 failed=0" "functional wrapper summary"

index_list_output="$("${INDEX_SCRIPT}" --list --only safety-smoke)"
assert_contains "${index_list_output}" "safety-smoke" "regression index list includes scenario"
assert_contains "${index_list_output}" "wrapper: safety-smoke.sh" "regression index wrapper marker"
assert_contains "${index_list_output}" "[demo:safety-smoke] PASS safety-prompt-injection-block" "regression index expected marker"

echo "safety-smoke demo tests passed"
