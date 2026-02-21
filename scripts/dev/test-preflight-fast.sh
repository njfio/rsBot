#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PREFLIGHT="${SCRIPT_DIR}/preflight-fast.sh"

assert_equals() {
  local expected="$1"
  local actual="$2"
  local label="$3"
  if [[ "${expected}" != "${actual}" ]]; then
    echo "assertion failed (${label}): expected '${expected}' got '${actual}'" >&2
    exit 1
  fi
}

assert_file_contains() {
  local path="$1"
  local needle="$2"
  local label="$3"
  local content
  content="$(cat "${path}")"
  if [[ "${content}" != *"${needle}"* ]]; then
    echo "assertion failed (${label}): expected '${needle}' in ${path}" >&2
    echo "actual content: ${content}" >&2
    exit 1
  fi
}

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

roadmap_pass="${tmp_dir}/roadmap-pass.sh"
roadmap_fail="${tmp_dir}/roadmap-fail.sh"
guard_pass="${tmp_dir}/guard-pass.sh"
guard_fail="${tmp_dir}/guard-fail.sh"
fast_validate="${tmp_dir}/fast-validate.sh"

cat >"${roadmap_pass}" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" > "${TEST_ROADMAP_ARGS_FILE}"
exit 0
EOF

cat >"${roadmap_fail}" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" > "${TEST_ROADMAP_ARGS_FILE}"
exit 7
EOF

cat >"${fast_validate}" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" > "${TEST_FAST_ARGS_FILE}"
touch "${TEST_FAST_CALLED_FILE}"
exit 0
EOF

cat >"${guard_pass}" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" > "${TEST_GUARD_ARGS_FILE}"
touch "${TEST_GUARD_CALLED_FILE}"
exit 0
EOF

cat >"${guard_fail}" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" > "${TEST_GUARD_ARGS_FILE}"
touch "${TEST_GUARD_CALLED_FILE}"
exit 9
EOF

chmod +x "${roadmap_pass}" "${roadmap_fail}" "${guard_pass}" "${guard_fail}" "${fast_validate}"

roadmap_args_file="${tmp_dir}/roadmap-args.txt"
guard_args_file="${tmp_dir}/guard-args.txt"
guard_called_file="${tmp_dir}/guard-called.txt"
fast_args_file="${tmp_dir}/fast-args.txt"
fast_called_file="${tmp_dir}/fast-called.txt"
export TEST_ROADMAP_ARGS_FILE="${roadmap_args_file}"
export TEST_GUARD_ARGS_FILE="${guard_args_file}"
export TEST_GUARD_CALLED_FILE="${guard_called_file}"
export TEST_FAST_ARGS_FILE="${fast_args_file}"
export TEST_FAST_CALLED_FILE="${fast_called_file}"

# Functional: success path runs roadmap check, guard, then forwards args.
TAU_ROADMAP_SYNC_BIN="${roadmap_pass}" \
TAU_PANIC_UNSAFE_GUARD_BIN="${guard_pass}" \
TAU_FAST_VALIDATE_BIN="${fast_validate}" \
"${PREFLIGHT}" --check-only --base origin/master

assert_file_contains "${roadmap_args_file}" "--check --quiet" "roadmap args"
if [[ ! -f "${guard_called_file}" ]]; then
  echo "assertion failed (success path): guard should be called" >&2
  exit 1
fi
assert_file_contains "${fast_args_file}" "--check-only --base origin/master" "passthrough args"
if [[ ! -f "${fast_called_file}" ]]; then
  echo "assertion failed (success path): fast-validate should be called" >&2
  exit 1
fi

# Regression: roadmap failure fails closed and does not call guard/fast-validate.
rm -f "${guard_called_file}"
rm -f "${fast_called_file}"
set +e
TAU_ROADMAP_SYNC_BIN="${roadmap_fail}" \
TAU_PANIC_UNSAFE_GUARD_BIN="${guard_pass}" \
TAU_FAST_VALIDATE_BIN="${fast_validate}" \
"${PREFLIGHT}" --check-only >/dev/null 2>&1
status=$?
set -e
assert_equals "7" "${status}" "roadmap failure exit code"
if [[ -f "${guard_called_file}" ]]; then
  echo "assertion failed (fail-closed): guard must not run on roadmap failure" >&2
  exit 1
fi
if [[ -f "${fast_called_file}" ]]; then
  echo "assertion failed (fail-closed): fast-validate must not run on roadmap failure" >&2
  exit 1
fi

# Regression: guard failure fails closed and does not call fast-validate.
rm -f "${guard_called_file}"
rm -f "${fast_called_file}"
set +e
TAU_ROADMAP_SYNC_BIN="${roadmap_pass}" \
TAU_PANIC_UNSAFE_GUARD_BIN="${guard_fail}" \
TAU_FAST_VALIDATE_BIN="${fast_validate}" \
"${PREFLIGHT}" --check-only >/dev/null 2>&1
status=$?
set -e
assert_equals "9" "${status}" "guard failure exit code"
if [[ ! -f "${guard_called_file}" ]]; then
  echo "assertion failed (guard failure): guard should run before failure" >&2
  exit 1
fi
if [[ -f "${fast_called_file}" ]]; then
  echo "assertion failed (guard failure): fast-validate must not run on guard failure" >&2
  exit 1
fi

echo "preflight-fast tests passed"
