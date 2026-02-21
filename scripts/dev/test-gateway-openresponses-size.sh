#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
ROOT_MODULE="${REPO_ROOT}/crates/tau-gateway/src/gateway_openresponses.rs"
EVENTS_MODULE="${REPO_ROOT}/crates/tau-gateway/src/gateway_openresponses/events_status.rs"
MAX_LINES=1450

if [[ ! -f "${ROOT_MODULE}" ]]; then
  echo "assertion failed (root module exists): ${ROOT_MODULE}" >&2
  exit 1
fi

if [[ ! -f "${EVENTS_MODULE}" ]]; then
  echo "assertion failed (events module exists): ${EVENTS_MODULE}" >&2
  exit 1
fi

line_count="$(wc -l < "${ROOT_MODULE}")"
if (( line_count > MAX_LINES )); then
  echo "assertion failed (gateway_openresponses.rs size): ${line_count} > ${MAX_LINES}" >&2
  exit 1
fi

if ! rg -q '^mod events_status;' "${ROOT_MODULE}"; then
  echo "assertion failed (events status module wiring): missing 'mod events_status;'" >&2
  exit 1
fi

if ! rg -q '^mod status_runtime;' "${ROOT_MODULE}"; then
  echo "assertion failed (status runtime module wiring): missing 'mod status_runtime;'" >&2
  exit 1
fi

if ! rg -q '^mod compat_state_runtime;' "${ROOT_MODULE}"; then
  echo "assertion failed (compat state runtime module wiring): missing 'mod compat_state_runtime;'" >&2
  exit 1
fi

echo "gateway-openresponses size guard passed (${line_count} <= ${MAX_LINES})"
