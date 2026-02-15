#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
cd "${REPO_ROOT}"

INSTALL_SH="${SCRIPT_DIR}/install-tau.sh"
UPDATE_SH="${SCRIPT_DIR}/update-tau.sh"
INSTALL_PS1="${SCRIPT_DIR}/install-tau.ps1"
UPDATE_PS1="${SCRIPT_DIR}/update-tau.ps1"

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

assert_file_exists() {
  local path="$1"
  local label="$2"
  if [[ ! -f "${path}" ]]; then
    echo "assertion failed (${label}): expected file '${path}' to exist" >&2
    exit 1
  fi
}

sha256_file() {
  local file_path="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "${file_path}" | awk '{print $1}'
    return
  fi
  shasum -a 256 "${file_path}" | awk '{print $1}'
}

create_linux_release_fixture() {
  local fixture_root="$1"
  local version="$2"
  local marker="$3"
  local platform="linux-amd64"
  local binary_name="tau-coding-agent-${platform}"
  local archive_name="${binary_name}.tar.gz"
  local payload_dir="${fixture_root}/payload/${version}"
  local release_dir="${fixture_root}/download/${version}"

  mkdir -p "${payload_dir}" "${release_dir}"
  cat > "${payload_dir}/${binary_name}" <<EOF
#!/usr/bin/env bash
if [[ "\${1:-}" == "--help" ]]; then
  echo "${marker}"
  exit 0
fi
echo "${marker} \$*"
EOF
  chmod +x "${payload_dir}/${binary_name}"
  tar -czf "${release_dir}/${archive_name}" -C "${payload_dir}" "${binary_name}"
  sha256_file "${release_dir}/${archive_name}" > "${release_dir}/${archive_name}.sha256"
}

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

release_root="${tmp_dir}/releases"
create_linux_release_fixture "${release_root}" "v9.9.9" "fixture-v9.9.9"
create_linux_release_fixture "${release_root}" "v9.9.10" "fixture-v9.9.10"
create_linux_release_fixture "${release_root}" "v9.9.11" "fixture-v9.9.11"

# Unit: resolved target metadata should map canonical os/arch aliases.
target_output="$(TAU_INSTALL_TEST_OS=linux TAU_INSTALL_TEST_ARCH=amd64 "${INSTALL_SH}" --version v9.9.9 --print-target)"
assert_contains "${target_output}" "platform=linux-amd64" "print-target platform"
assert_contains "${target_output}" "archive=tau-coding-agent-linux-amd64.tar.gz" "print-target archive"

# Functional: install should fetch fixture release and place executable in destination.
install_dir="${tmp_dir}/bin"
TAU_RELEASE_BASE_URL="file://${release_root}/download" \
TAU_INSTALL_TEST_OS=linux \
TAU_INSTALL_TEST_ARCH=amd64 \
"${INSTALL_SH}" --version v9.9.9 --install-dir "${install_dir}"
assert_file_exists "${install_dir}/tau-coding-agent" "installed binary"
help_output="$("${install_dir}/tau-coding-agent" --help)"
assert_contains "${help_output}" "fixture-v9.9.9" "installed binary payload marker"

# Integration: update wrapper should route through installer update mode and replace binary.
TAU_RELEASE_BASE_URL="file://${release_root}/download" \
TAU_INSTALL_TEST_OS=linux \
TAU_INSTALL_TEST_ARCH=amd64 \
"${UPDATE_SH}" --version v9.9.10 --install-dir "${install_dir}"
updated_help_output="$("${install_dir}/tau-coding-agent" --help)"
assert_contains "${updated_help_output}" "fixture-v9.9.10" "updated binary payload marker"

# Regression: corrupted checksum must fail with checksum_mismatch reason code.
echo "0000000000000000000000000000000000000000000000000000000000000000  tau-coding-agent-linux-amd64.tar.gz" \
  > "${release_root}/download/v9.9.11/tau-coding-agent-linux-amd64.tar.gz.sha256"
set +e
corrupt_output="$(TAU_RELEASE_BASE_URL="file://${release_root}/download" \
  TAU_INSTALL_TEST_OS=linux \
  TAU_INSTALL_TEST_ARCH=amd64 \
  "${INSTALL_SH}" --version v9.9.11 --install-dir "${tmp_dir}/corrupt-bin" 2>&1)"
corrupt_exit_code=$?
set -e
if [[ ${corrupt_exit_code} -eq 0 ]]; then
  echo "assertion failed (checksum regression): install unexpectedly succeeded" >&2
  exit 1
fi
assert_contains "${corrupt_output}" "\"reason_code\":\"checksum_mismatch\"" "checksum mismatch reason code"

# Regression: update wrapper should fail when install target is missing.
set +e
missing_update_output="$(TAU_RELEASE_BASE_URL="file://${release_root}/download" \
  TAU_INSTALL_TEST_OS=linux \
  TAU_INSTALL_TEST_ARCH=amd64 \
  "${UPDATE_SH}" --version v9.9.10 --install-dir "${tmp_dir}/missing-update" 2>&1)"
missing_update_exit_code=$?
set -e
if [[ ${missing_update_exit_code} -eq 0 ]]; then
  echo "assertion failed (update missing target): update unexpectedly succeeded" >&2
  exit 1
fi
assert_contains "${missing_update_output}" "\"reason_code\":\"update_target_missing\"" "update missing target reason code"

# PowerShell script smoke tests (executed when pwsh is available on host).
if command -v pwsh >/dev/null 2>&1; then
  ps_target_output="$(TAU_INSTALL_TEST_OS=windows TAU_INSTALL_TEST_ARCH=amd64 \
    pwsh -NoProfile -File "${INSTALL_PS1}" -Version v9.9.9 -PrintTarget)"
  assert_contains "${ps_target_output}" "platform=windows-amd64" "PowerShell print-target platform"

  ps_dry_run_output="$(TAU_INSTALL_TEST_OS=windows TAU_INSTALL_TEST_ARCH=amd64 \
    pwsh -NoProfile -File "${UPDATE_PS1}" -Version v9.9.9 -DryRun 2>&1)"
  assert_contains "${ps_dry_run_output}" "\"reason_code\":\"dry_run\"" "PowerShell dry-run reason code"
fi

echo "release install/update helper tests passed"
