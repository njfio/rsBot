#!/usr/bin/env bash
set -euo pipefail

APP_NAME="tau-coding-agent"
SCRIPT_NAME="$(basename "$0")"

REPO_SLUG="${TAU_RELEASE_REPO:-njfio/Tau}"
RELEASE_BASE_URL="${TAU_RELEASE_BASE_URL:-https://github.com/${REPO_SLUG}/releases/download}"
LATEST_BASE_URL="${TAU_RELEASE_LATEST_URL:-https://github.com/${REPO_SLUG}/releases/latest/download}"

INSTALL_DIR="${TAU_INSTALL_DIR:-${HOME}/.local/bin}"
BINARY_NAME="${TAU_BINARY_NAME:-${APP_NAME}}"

VERSION=""
UPDATE_MODE="false"
DRY_RUN="false"
FORCE_MODE="false"
VERIFY_CHECKSUMS="true"
PRINT_TARGET="false"

json_escape() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  printf '%s' "${value}"
}

log_event() {
  local level="$1"
  local reason_code="$2"
  local message="$3"
  printf '{"ts":"%s","component":"release-installer","level":"%s","reason_code":"%s","message":"%s"}\n' \
    "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    "${level}" \
    "${reason_code}" \
    "$(json_escape "${message}")"
}

fail_reason() {
  local reason_code="$1"
  local message="$2"
  log_event "error" "${reason_code}" "${message}" >&2
  exit 1
}

usage() {
  cat <<EOF
Usage: ${SCRIPT_NAME} [options]

Install or update ${APP_NAME} from GitHub Releases.

Options:
  --version <tag>      Install a specific release tag (for example: v0.4.2).
                       Default is latest release.
  --install-dir <dir>  Destination directory (default: ${INSTALL_DIR}).
  --binary-name <name> Destination binary filename (default: ${BINARY_NAME}).
  --update             Update an existing install in place.
  --force              Overwrite destination for non-update installs.
  --dry-run            Resolve URLs/targets and exit without downloading.
  --no-verify          Skip SHA256 checksum verification.
  --print-target       Print resolved platform/archive metadata and exit.
  --help, -h           Show this message.

Environment overrides:
  TAU_RELEASE_REPO, TAU_RELEASE_BASE_URL, TAU_RELEASE_LATEST_URL
  TAU_INSTALL_DIR, TAU_BINARY_NAME
  TAU_INSTALL_TEST_OS, TAU_INSTALL_TEST_ARCH (test-only overrides)
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      shift
      [[ $# -gt 0 ]] || fail_reason "argument_missing" "--version requires a value"
      VERSION="$1"
      ;;
    --install-dir)
      shift
      [[ $# -gt 0 ]] || fail_reason "argument_missing" "--install-dir requires a value"
      INSTALL_DIR="$1"
      ;;
    --binary-name)
      shift
      [[ $# -gt 0 ]] || fail_reason "argument_missing" "--binary-name requires a value"
      BINARY_NAME="$1"
      ;;
    --update)
      UPDATE_MODE="true"
      ;;
    --force)
      FORCE_MODE="true"
      ;;
    --dry-run)
      DRY_RUN="true"
      ;;
    --no-verify)
      VERIFY_CHECKSUMS="false"
      ;;
    --print-target)
      PRINT_TARGET="true"
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      fail_reason "argument_unknown" "unknown argument: $1"
      ;;
  esac
  shift
done

resolve_os_slug() {
  local raw_os
  raw_os="$(printf '%s' "${TAU_INSTALL_TEST_OS:-$(uname -s)}" | tr '[:upper:]' '[:lower:]')"
  case "${raw_os}" in
    linux|linux*)
      printf '%s\n' "linux"
      ;;
    darwin|darwin*|macos|macos*)
      printf '%s\n' "macos"
      ;;
    *)
      fail_reason "unsupported_os" "unsupported operating system: ${raw_os}"
      ;;
  esac
}

resolve_arch_slug() {
  local raw_arch
  raw_arch="$(printf '%s' "${TAU_INSTALL_TEST_ARCH:-$(uname -m)}" | tr '[:upper:]' '[:lower:]')"
  case "${raw_arch}" in
    x86_64|amd64)
      printf '%s\n' "amd64"
      ;;
    aarch64|arm64)
      printf '%s\n' "arm64"
      ;;
    *)
      fail_reason "unsupported_arch" "unsupported architecture: ${raw_arch}"
      ;;
  esac
}

download_to() {
  local source_url="$1"
  local output_path="$2"
  if command -v curl >/dev/null 2>&1; then
    curl --fail --silent --show-error --location --retry 3 --retry-delay 2 \
      --output "${output_path}" "${source_url}"
    return
  fi
  if command -v wget >/dev/null 2>&1; then
    wget --quiet -O "${output_path}" "${source_url}"
    return
  fi
  fail_reason "download_tool_missing" "missing curl/wget required for download"
}

sha256_file() {
  local file_path="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "${file_path}" | awk '{print $1}'
    return
  fi
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "${file_path}" | awk '{print $1}'
    return
  fi
  if command -v openssl >/dev/null 2>&1; then
    openssl dgst -sha256 "${file_path}" | awk '{print $2}'
    return
  fi
  fail_reason "checksum_tool_missing" "missing sha256sum/shasum/openssl for checksum verification"
}

OS_SLUG="$(resolve_os_slug)"
ARCH_SLUG="$(resolve_arch_slug)"
PLATFORM="${OS_SLUG}-${ARCH_SLUG}"
ARCHIVE_NAME="${APP_NAME}-${PLATFORM}.tar.gz"

if [[ -n "${VERSION}" ]]; then
  ARCHIVE_URL="${RELEASE_BASE_URL}/${VERSION}/${ARCHIVE_NAME}"
else
  ARCHIVE_URL="${LATEST_BASE_URL}/${ARCHIVE_NAME}"
fi
CHECKSUM_URL="${ARCHIVE_URL}.sha256"

if [[ "${PRINT_TARGET}" == "true" ]]; then
  printf '%s\n' "platform=${PLATFORM}"
  printf '%s\n' "archive=${ARCHIVE_NAME}"
  printf '%s\n' "archive_url=${ARCHIVE_URL}"
  printf '%s\n' "checksum_url=${CHECKSUM_URL}"
  exit 0
fi

if [[ "${DRY_RUN}" == "true" ]]; then
  log_event "info" "dry_run" "resolved install metadata only"
  printf '%s\n' "platform=${PLATFORM}"
  printf '%s\n' "archive_url=${ARCHIVE_URL}"
  printf '%s\n' "install_dir=${INSTALL_DIR}"
  exit 0
fi

WORK_DIR="$(mktemp -d)"
EXTRACT_DIR="${WORK_DIR}/extract"
ARCHIVE_PATH="${WORK_DIR}/${ARCHIVE_NAME}"
CHECKSUM_PATH="${WORK_DIR}/${ARCHIVE_NAME}.sha256"
DESTINATION_PATH="${INSTALL_DIR}/${BINARY_NAME}"
BACKUP_PATH=""

cleanup() {
  rm -rf "${WORK_DIR}"
}
trap cleanup EXIT

if [[ "${UPDATE_MODE}" == "true" && ! -f "${DESTINATION_PATH}" ]]; then
  fail_reason "update_target_missing" "update requested but destination does not exist: ${DESTINATION_PATH}"
fi

if [[ "${UPDATE_MODE}" != "true" && -f "${DESTINATION_PATH}" && "${FORCE_MODE}" != "true" ]]; then
  fail_reason "destination_exists" "destination exists; rerun with --force or --update: ${DESTINATION_PATH}"
fi

log_event "info" "download_started" "downloading release archive"
download_to "${ARCHIVE_URL}" "${ARCHIVE_PATH}" || fail_reason "download_failed" "failed to download archive: ${ARCHIVE_URL}"

if [[ "${VERIFY_CHECKSUMS}" == "true" ]]; then
  log_event "info" "checksum_fetch_started" "downloading checksum manifest"
  download_to "${CHECKSUM_URL}" "${CHECKSUM_PATH}" || fail_reason "checksum_download_failed" "failed to download checksum: ${CHECKSUM_URL}"
  expected_hash="$(awk '{print $1}' "${CHECKSUM_PATH}" | head -n 1 | tr '[:upper:]' '[:lower:]')"
  if [[ ! "${expected_hash}" =~ ^[a-f0-9]{64}$ ]]; then
    fail_reason "checksum_manifest_invalid" "checksum manifest did not contain a valid SHA256"
  fi
  actual_hash="$(sha256_file "${ARCHIVE_PATH}" | tr '[:upper:]' '[:lower:]')"
  if [[ "${actual_hash}" != "${expected_hash}" ]]; then
    fail_reason "checksum_mismatch" "archive checksum mismatch"
  fi
  log_event "info" "checksum_verified" "checksum verification succeeded"
else
  log_event "warn" "checksum_verification_skipped" "checksum verification disabled by operator"
fi

mkdir -p "${EXTRACT_DIR}"
tar -xzf "${ARCHIVE_PATH}" -C "${EXTRACT_DIR}" || fail_reason "archive_extract_failed" "failed to extract archive"

PACKAGED_BINARY="${APP_NAME}-${PLATFORM}"
SOURCE_PATH="$(find "${EXTRACT_DIR}" -maxdepth 2 -type f -name "${PACKAGED_BINARY}" | head -n 1)"
if [[ -z "${SOURCE_PATH}" ]]; then
  fail_reason "binary_not_found" "packaged binary not found in archive: ${PACKAGED_BINARY}"
fi

mkdir -p "${INSTALL_DIR}"
if [[ -f "${DESTINATION_PATH}" ]]; then
  BACKUP_PATH="${DESTINATION_PATH}.bak.$$"
  cp "${DESTINATION_PATH}" "${BACKUP_PATH}"
fi

cp "${SOURCE_PATH}" "${DESTINATION_PATH}" || fail_reason "install_copy_failed" "failed to copy binary to destination"
chmod +x "${DESTINATION_PATH}" || fail_reason "install_chmod_failed" "failed to make destination executable"

if ! "${DESTINATION_PATH}" --help >/dev/null 2>&1; then
  if [[ -n "${BACKUP_PATH}" && -f "${BACKUP_PATH}" ]]; then
    cp "${BACKUP_PATH}" "${DESTINATION_PATH}"
    chmod +x "${DESTINATION_PATH}" || true
  fi
  fail_reason "smoke_test_failed" "installed binary failed --help smoke test"
fi

if [[ -n "${BACKUP_PATH}" && -f "${BACKUP_PATH}" ]]; then
  rm -f "${BACKUP_PATH}"
fi

if [[ "${UPDATE_MODE}" == "true" ]]; then
  log_event "info" "update_complete" "update completed successfully"
else
  log_event "info" "install_complete" "install completed successfully"
fi
printf '%s\n' "install_path=${DESTINATION_PATH}"
