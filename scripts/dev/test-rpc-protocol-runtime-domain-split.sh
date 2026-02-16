#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

runtime_file="crates/tau-runtime/src/rpc_protocol_runtime.rs"
runtime_dir="crates/tau-runtime/src/rpc_protocol_runtime"

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "${haystack}" != *"${needle}"* ]]; then
    echo "assertion failed (${label}): expected to find '${needle}'" >&2
    exit 1
  fi
}

assert_not_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "${haystack}" == *"${needle}"* ]]; then
    echo "assertion failed (${label}): expected to NOT find '${needle}'" >&2
    exit 1
  fi
}

line_count="$(wc -l < "${runtime_file}" | tr -d ' ')"
if (( line_count >= 2400 )); then
  echo "assertion failed (line budget): expected ${runtime_file} < 2400 lines, got ${line_count}" >&2
  exit 1
fi

for file in \
  "${runtime_dir}/parsing.rs" \
  "${runtime_dir}/dispatch.rs" \
  "${runtime_dir}/transport.rs"; do
  if [[ ! -f "${file}" ]]; then
    echo "assertion failed (module file): missing ${file}" >&2
    exit 1
  fi
done

runtime_contents="$(cat "${runtime_file}")"

assert_contains "${runtime_contents}" "mod parsing;" "module marker: parsing"
assert_contains "${runtime_contents}" "mod dispatch;" "module marker: dispatch"
assert_contains "${runtime_contents}" "mod transport;" "module marker: transport"

assert_not_contains "${runtime_contents}" "fn dispatch_rpc_frame_for_serve(" "moved helper: dispatch"
assert_not_contains "${runtime_contents}" "fn dispatch_rpc_ndjson_input_impl(" "moved helper: transport"
assert_not_contains "${runtime_contents}" "fn parse_rpc_frame_impl(" "moved helper: parsing"

echo "rpc-protocol-runtime-domain-split tests passed"
